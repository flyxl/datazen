//! Portable dashboard persistence rules; the host owns the rows.
//!
//! A host stores dashboards and their widgets as rows ([`DashboardRecord`]
//! and [`WidgetRecord`]) and provides them through [`DashboardStore`]. What
//! a dashboard *means* — which widgets it owns, when a widget is removed,
//! how a refresh policy is normalised before it is written — is decided
//! here, so every host persists the same dashboard the same way.

use super::records::{DashboardRecord, WidgetRecord};
use super::types::{
    parse_refresh_mode, parse_view_mode, refresh_mode_str, view_mode_str, AlertRule, ChartConfig,
    Dashboard, DashboardLayout, DashboardWidget, DashboardWorkflowRef, RefreshPolicy, WidgetLayout,
};
use chrono::Utc;
use datazen_platform_api::ports::scope::OrganizationScope;
use std::collections::HashSet;

/// Why a dashboard operation failed.
#[derive(Debug, thiserror::Error)]
pub enum DashboardStoreError {
    #[error("Store error: {0}")]
    Db(String),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Not found: {0}")]
    NotFound(String),
}

/// The rows a host stores dashboards and widgets as.
///
/// The desktop host implements this over its SQLite store; another host may
/// implement it over its own persistence. The trait only moves rows — the
/// rules that decide what a dashboard contains stay in this module.
///
/// ## 组织作用域
///
/// 每个方法都带一个 [`OrganizationScope`]：存储实现据此把「本组织的数据」与
/// 「别的组织的数据」分开——多租户宿主**不需要**为每个组织准备一个存储实例。
///
/// 越组织访问一律按「不存在」处理（返回空列表 / `NotFound`），**不得**返回
/// 「无权限」类错误：后者会泄漏「存在但无权」（CM-05）。
pub trait DashboardStore: Send + Sync {
    /// Every stored dashboard row of the organization.
    fn list_dashboard_rows(
        &self,
        scope: &OrganizationScope,
    ) -> Result<Vec<DashboardRecord>, DashboardStoreError>;

    /// One stored dashboard row of the organization.
    fn dashboard_row(
        &self,
        scope: &OrganizationScope,
        id: &str,
    ) -> Result<DashboardRecord, DashboardStoreError>;

    /// Insert or update a dashboard row.
    fn upsert_dashboard_row(
        &self,
        scope: &OrganizationScope,
        row: &DashboardRecord,
    ) -> Result<(), DashboardStoreError>;

    /// Remove a dashboard row.
    fn delete_dashboard_row(
        &self,
        scope: &OrganizationScope,
        id: &str,
    ) -> Result<(), DashboardStoreError>;

    /// Pause or resume the automatic refresh of a dashboard.
    fn set_dashboard_refresh_paused(
        &self,
        scope: &OrganizationScope,
        id: &str,
        paused: bool,
    ) -> Result<(), DashboardStoreError>;

    /// The widget rows of a dashboard.
    fn list_widget_rows(
        &self,
        scope: &OrganizationScope,
        dashboard_id: &str,
    ) -> Result<Vec<WidgetRecord>, DashboardStoreError>;

    /// One stored widget row.
    fn widget_row(
        &self,
        scope: &OrganizationScope,
        id: &str,
    ) -> Result<WidgetRecord, DashboardStoreError>;

    /// Insert or update a widget row.
    fn upsert_widget_row(
        &self,
        scope: &OrganizationScope,
        row: &WidgetRecord,
    ) -> Result<(), DashboardStoreError>;

    /// Remove a widget row.
    fn delete_widget_row(
        &self,
        scope: &OrganizationScope,
        id: &str,
    ) -> Result<(), DashboardStoreError>;

    /// The dashboards and widgets that reference a workflow.
    fn find_workflow_refs(
        &self,
        scope: &OrganizationScope,
        workflow_id: &str,
    ) -> Result<Vec<DashboardWorkflowRef>, DashboardStoreError>;
}

/// The stored spelling of a widget's view mode.
fn view_mode_of(widget: &DashboardWidget) -> String {
    view_mode_str(widget.view_mode).into()
}

/// The stored spelling of a widget's refresh mode.
fn refresh_mode_of(refresh: &RefreshPolicy) -> String {
    refresh_mode_str(refresh.mode).into()
}

/// Turn a stored widget row back into a widget.
pub fn widget_from_record(record: &WidgetRecord) -> Result<DashboardWidget, DashboardStoreError> {
    let chart_config = record
        .chart_config_json
        .as_ref()
        .map(|json| {
            serde_json::from_str::<ChartConfig>(json)
                .map_err(|e| DashboardStoreError::Parse(e.to_string()))
        })
        .transpose()?;
    let alert = record
        .alert_json
        .as_ref()
        .map(|json| {
            serde_json::from_str::<AlertRule>(json)
                .map_err(|e| DashboardStoreError::Parse(e.to_string()))
        })
        .transpose()?;
    Ok(DashboardWidget {
        id: record.id.clone(),
        title: record.title.clone(),
        workflow_id: record.workflow_id.clone(),
        view_mode: parse_view_mode(&record.view_mode).ok_or_else(|| {
            DashboardStoreError::Parse(format!("invalid view_mode: {}", record.view_mode))
        })?,
        chart_config,
        layout: WidgetLayout {
            x: record.layout_x,
            y: record.layout_y,
            w: record.layout_w,
            h: record.layout_h,
        },
        refresh: RefreshPolicy {
            mode: parse_refresh_mode(&record.refresh_mode).ok_or_else(|| {
                DashboardStoreError::Parse(format!("invalid refresh_mode: {}", record.refresh_mode))
            })?,
            refresh_sec: record.refresh_sec,
        },
        alert,
        enabled: record.enabled,
    })
}

/// Turn a widget into the row a host stores it as.
pub fn widget_to_record(
    widget: &DashboardWidget,
    dashboard_id: &str,
    sort_order: i32,
) -> WidgetRecord {
    let mut refresh = widget.refresh.clone();
    refresh.normalize();
    WidgetRecord {
        id: widget.id.clone(),
        dashboard_id: dashboard_id.to_string(),
        title: widget.title.clone(),
        workflow_id: widget.workflow_id.clone(),
        view_mode: view_mode_of(widget),
        chart_config_json: widget
            .chart_config
            .as_ref()
            .and_then(|c| serde_json::to_string(c).ok()),
        layout_x: widget.layout.x,
        layout_y: widget.layout.y,
        layout_w: widget.layout.w,
        layout_h: widget.layout.h,
        refresh_mode: refresh_mode_of(&refresh),
        refresh_sec: refresh.refresh_sec,
        alert_json: widget
            .alert
            .as_ref()
            .and_then(|a| serde_json::to_string(a).ok()),
        enabled: widget.enabled,
        sort_order,
        created_at: Utc::now().to_rfc3339(),
        updated_at: Utc::now().to_rfc3339(),
    }
}

/// Turn a dashboard into the row a host stores it as.
pub fn dashboard_to_record(dashboard: &Dashboard) -> DashboardRecord {
    DashboardRecord {
        id: dashboard.id.clone(),
        name: dashboard.name.clone(),
        created_at: dashboard.created_at.clone(),
        updated_at: dashboard.updated_at.clone(),
        layout_cols: dashboard.layout.cols,
        layout_row_height: dashboard.layout.row_height,
        enabled: dashboard.enabled,
        refresh_paused: dashboard.refresh_paused,
    }
}

/// Turn a stored dashboard row and its widgets back into a dashboard.
pub fn dashboard_from_record(record: &DashboardRecord, widgets: Vec<DashboardWidget>) -> Dashboard {
    Dashboard {
        id: record.id.clone(),
        name: record.name.clone(),
        created_at: record.created_at.clone(),
        updated_at: record.updated_at.clone(),
        layout: DashboardLayout {
            cols: record.layout_cols,
            row_height: record.layout_row_height,
        },
        widgets,
        enabled: record.enabled,
        refresh_paused: record.refresh_paused,
    }
}

/// The widgets of a dashboard, in their stored order.
fn widgets_of(
    scope: &OrganizationScope,
    store: &dyn DashboardStore,
    dashboard_id: &str,
) -> Result<Vec<DashboardWidget>, DashboardStoreError> {
    store
        .list_widget_rows(scope, dashboard_id)?
        .iter()
        .map(widget_from_record)
        .collect()
}

/// Every stored dashboard of the organization, each with its widgets.
pub fn list_dashboards(
    scope: &OrganizationScope,
    store: &dyn DashboardStore,
) -> Result<Vec<Dashboard>, DashboardStoreError> {
    let records = store.list_dashboard_rows(scope)?;
    let mut dashboards = Vec::with_capacity(records.len());
    for record in records {
        let widgets = widgets_of(scope, store, &record.id)?;
        dashboards.push(dashboard_from_record(&record, widgets));
    }
    Ok(dashboards)
}

/// One stored dashboard of the organization, with its widgets.
pub fn get_dashboard(
    scope: &OrganizationScope,
    store: &dyn DashboardStore,
    id: &str,
) -> Result<Dashboard, DashboardStoreError> {
    let record = store.dashboard_row(scope, id)?;
    let widgets = widgets_of(scope, store, id)?;
    Ok(dashboard_from_record(&record, widgets))
}

/// Store a dashboard and its widgets.
///
/// The refresh policy of every widget is normalised first, the widgets that
/// are gone are removed, and the widgets that stay keep the creation time
/// they were first stored with.
pub fn save_dashboard(
    scope: &OrganizationScope,
    store: &dyn DashboardStore,
    mut dashboard: Dashboard,
) -> Result<(), DashboardStoreError> {
    for widget in &mut dashboard.widgets {
        widget.refresh.normalize();
    }
    dashboard.updated_at = Utc::now().to_rfc3339();

    let existing = store
        .list_widget_rows(scope, &dashboard.id)
        .unwrap_or_default();
    let existing_ids: HashSet<_> = existing.iter().map(|w| w.id.as_str()).collect();
    let new_ids: HashSet<_> = dashboard.widgets.iter().map(|w| w.id.as_str()).collect();

    store.upsert_dashboard_row(scope, &dashboard_to_record(&dashboard))?;

    for (idx, widget) in dashboard.widgets.iter().enumerate() {
        let mut record = widget_to_record(widget, &dashboard.id, idx as i32);
        if let Ok(prev) = store.widget_row(scope, &widget.id) {
            record.created_at = prev.created_at;
        }
        record.updated_at = dashboard.updated_at.clone();
        store.upsert_widget_row(scope, &record)?;
    }

    for id in existing_ids.difference(&new_ids) {
        store.delete_widget_row(scope, id)?;
    }

    Ok(())
}

/// Remove a stored dashboard.
pub fn delete_dashboard(
    scope: &OrganizationScope,
    store: &dyn DashboardStore,
    id: &str,
) -> Result<(), DashboardStoreError> {
    store.delete_dashboard_row(scope, id)
}

/// Pause or resume the automatic refresh of a stored dashboard.
pub fn set_dashboard_refresh_paused(
    scope: &OrganizationScope,
    store: &dyn DashboardStore,
    id: &str,
    paused: bool,
) -> Result<(), DashboardStoreError> {
    store.set_dashboard_refresh_paused(scope, id, paused)
}

/// The dashboards and widgets that reference a workflow.
pub fn find_workflow_refs(
    scope: &OrganizationScope,
    store: &dyn DashboardStore,
    workflow_id: &str,
) -> Result<Vec<DashboardWorkflowRef>, DashboardStoreError> {
    store.find_workflow_refs(scope, workflow_id)
}

/// Give an imported dashboard a new id when the id is already taken locally.
///
/// `existing` 必须是**同一组织**里读到的仪表板：id 是否冲突只在组织内成立。
pub fn assign_import_id(
    scope: &OrganizationScope,
    dashboard: &mut Dashboard,
    existing: &[Dashboard],
) {
    if existing.iter().any(|d| d.id == dashboard.id) {
        tracing::debug!(
            organization_id = scope.organization_id().as_str(),
            dashboard_id = %dashboard.id,
            "imported dashboard id already exists in this organization; assigning a new id"
        );
        dashboard.id = uuid::Uuid::new_v4().to_string();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dashboard::types::{
        ChartType, DashboardLayout, RefreshMode, RefreshPolicy, ViewMode, WidgetLayout,
    };
    use datazen_platform_api::id::OrganizationId;

    /// 一个**按组织分区**的内存存储：同一份数据里并存多个组织的数据，
    /// 读取时只返回作用域所属组织的那一部分。
    ///
    /// 这就是多租户宿主需要的形状——**同一个存储实现**上区分「本组织的数据」
    /// 与「别的组织的数据」，而不是每个组织一个存储实例。
    struct MemoryStore {
        dashboards: std::sync::Mutex<Vec<(OrganizationId, DashboardRecord)>>,
        widgets: std::sync::Mutex<Vec<(OrganizationId, WidgetRecord)>>,
    }

    impl MemoryStore {
        fn new() -> Self {
            Self {
                dashboards: std::sync::Mutex::new(Vec::new()),
                widgets: std::sync::Mutex::new(Vec::new()),
            }
        }
    }

    /// 某个组织的作用域。
    fn scope_of(organization: &str) -> OrganizationScope {
        OrganizationScope::new(OrganizationId::new(organization))
    }

    impl DashboardStore for MemoryStore {
        fn list_dashboard_rows(
            &self,
            scope: &OrganizationScope,
        ) -> Result<Vec<DashboardRecord>, DashboardStoreError> {
            Ok(self
                .dashboards
                .lock()
                .unwrap()
                .iter()
                .filter(|(owner, _)| owner == scope.organization_id())
                .map(|(_, row)| row.clone())
                .collect())
        }
        fn dashboard_row(
            &self,
            scope: &OrganizationScope,
            id: &str,
        ) -> Result<DashboardRecord, DashboardStoreError> {
            self.dashboards
                .lock()
                .unwrap()
                .iter()
                .find(|(owner, row)| owner == scope.organization_id() && row.id == id)
                .map(|(_, row)| row.clone())
                .ok_or_else(|| DashboardStoreError::NotFound(id.to_string()))
        }
        fn upsert_dashboard_row(
            &self,
            scope: &OrganizationScope,
            row: &DashboardRecord,
        ) -> Result<(), DashboardStoreError> {
            let mut rows = self.dashboards.lock().unwrap();
            match rows
                .iter_mut()
                .find(|(owner, existing)| owner == scope.organization_id() && existing.id == row.id)
            {
                Some((_, existing)) => *existing = row.clone(),
                None => rows.push((scope.organization_id().clone(), row.clone())),
            }
            Ok(())
        }
        fn delete_dashboard_row(
            &self,
            scope: &OrganizationScope,
            id: &str,
        ) -> Result<(), DashboardStoreError> {
            let mut rows = self.dashboards.lock().unwrap();
            rows.retain(|(owner, row)| !(owner == scope.organization_id() && row.id == id));
            Ok(())
        }
        fn set_dashboard_refresh_paused(
            &self,
            scope: &OrganizationScope,
            id: &str,
            paused: bool,
        ) -> Result<(), DashboardStoreError> {
            let mut rows = self.dashboards.lock().unwrap();
            let (_, row) = rows
                .iter_mut()
                .find(|(owner, row)| owner == scope.organization_id() && row.id == id)
                .ok_or_else(|| DashboardStoreError::NotFound(id.to_string()))?;
            row.refresh_paused = paused;
            Ok(())
        }
        fn list_widget_rows(
            &self,
            scope: &OrganizationScope,
            dashboard_id: &str,
        ) -> Result<Vec<WidgetRecord>, DashboardStoreError> {
            Ok(self
                .widgets
                .lock()
                .unwrap()
                .iter()
                .filter(|(owner, row)| {
                    owner == scope.organization_id() && row.dashboard_id == dashboard_id
                })
                .map(|(_, row)| row.clone())
                .collect())
        }
        fn widget_row(
            &self,
            scope: &OrganizationScope,
            id: &str,
        ) -> Result<WidgetRecord, DashboardStoreError> {
            self.widgets
                .lock()
                .unwrap()
                .iter()
                .find(|(owner, row)| owner == scope.organization_id() && row.id == id)
                .map(|(_, row)| row.clone())
                .ok_or_else(|| DashboardStoreError::NotFound(id.to_string()))
        }
        fn upsert_widget_row(
            &self,
            scope: &OrganizationScope,
            row: &WidgetRecord,
        ) -> Result<(), DashboardStoreError> {
            let mut rows = self.widgets.lock().unwrap();
            match rows
                .iter_mut()
                .find(|(owner, existing)| owner == scope.organization_id() && existing.id == row.id)
            {
                Some((_, existing)) => *existing = row.clone(),
                None => rows.push((scope.organization_id().clone(), row.clone())),
            }
            Ok(())
        }
        fn delete_widget_row(
            &self,
            scope: &OrganizationScope,
            id: &str,
        ) -> Result<(), DashboardStoreError> {
            let mut rows = self.widgets.lock().unwrap();
            rows.retain(|(owner, row)| !(owner == scope.organization_id() && row.id == id));
            Ok(())
        }
        fn find_workflow_refs(
            &self,
            _scope: &OrganizationScope,
            _workflow_id: &str,
        ) -> Result<Vec<DashboardWorkflowRef>, DashboardStoreError> {
            Ok(Vec::new())
        }
    }

    fn sample_widget(id: &str, workflow_id: &str) -> DashboardWidget {
        DashboardWidget {
            id: id.into(),
            title: "Widget".into(),
            workflow_id: workflow_id.into(),
            view_mode: ViewMode::Chart,
            chart_config: Some(ChartConfig {
                chart_type: ChartType::Bar,
                ..ChartConfig::default()
            }),
            layout: WidgetLayout {
                x: 0,
                y: 0,
                w: 6,
                h: 4,
            },
            refresh: RefreshPolicy {
                mode: RefreshMode::Manual,
                refresh_sec: None,
            },
            alert: None,
            enabled: true,
        }
    }

    fn sample_dashboard_with(id: &str, name: &str, widgets: Vec<DashboardWidget>) -> Dashboard {
        Dashboard {
            id: id.into(),
            name: name.into(),
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
            layout: DashboardLayout {
                cols: 12,
                row_height: 80,
            },
            widgets,
            enabled: true,
            refresh_paused: false,
        }
    }

    fn sample_dashboard(widgets: Vec<DashboardWidget>) -> Dashboard {
        sample_dashboard_with("dash-1", "Test Dashboard", widgets)
    }

    #[test]
    fn save_and_list_dashboard_roundtrip() {
        let store = MemoryStore::new();
        let scope = scope_of("org-a");
        let dashboard = sample_dashboard(vec![sample_widget("w1", "wf-1")]);
        save_dashboard(&scope, &store, dashboard.clone()).unwrap();
        let list = list_dashboards(&scope, &store).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, dashboard.id);
        assert_eq!(list[0].widgets[0].workflow_id, "wf-1");
    }

    #[test]
    fn save_dashboard_removes_the_widgets_that_are_gone() {
        let store = MemoryStore::new();
        let scope = scope_of("org-a");
        save_dashboard(
            &scope,
            &store,
            sample_dashboard(vec![
                sample_widget("w1", "wf-1"),
                sample_widget("w2", "wf-1"),
            ]),
        )
        .unwrap();
        save_dashboard(
            &scope,
            &store,
            sample_dashboard(vec![sample_widget("w1", "wf-1")]),
        )
        .unwrap();
        let loaded = get_dashboard(&scope, &store, "dash-1").unwrap();
        assert_eq!(loaded.widgets.len(), 1);
        assert_eq!(loaded.widgets[0].id, "w1");
    }

    #[test]
    fn save_dashboard_clamps_interval_refresh_sec() {
        let store = MemoryStore::new();
        let scope = scope_of("org-a");
        let mut widget = sample_widget("w1", "wf-1");
        widget.refresh = RefreshPolicy {
            mode: RefreshMode::Interval,
            refresh_sec: Some(5),
        };
        save_dashboard(&scope, &store, sample_dashboard(vec![widget])).unwrap();
        let loaded = get_dashboard(&scope, &store, "dash-1").unwrap();
        assert_eq!(loaded.widgets[0].refresh.refresh_sec, Some(30));
    }

    #[test]
    fn set_dashboard_refresh_paused_persists() {
        let store = MemoryStore::new();
        let scope = scope_of("org-a");
        save_dashboard(&scope, &store, sample_dashboard(vec![])).unwrap();
        set_dashboard_refresh_paused(&scope, &store, "dash-1", true).unwrap();
        assert!(
            get_dashboard(&scope, &store, "dash-1")
                .unwrap()
                .refresh_paused
        );
    }

    #[test]
    fn assign_import_id_keeps_a_free_id_and_replaces_a_taken_one() {
        let scope = scope_of("org-a");
        let mut dashboard = sample_dashboard(vec![]);
        let existing = vec![sample_dashboard(vec![])];
        assign_import_id(&scope, &mut dashboard, &[]);
        assert_eq!(dashboard.id, "dash-1");
        assign_import_id(&scope, &mut dashboard, &existing);
        assert_ne!(dashboard.id, "dash-1");
    }

    // ─── 组织隔离 ───

    /// 别的组织的仪表板对本组织**不存在**：列表为空、单条读不到（CM-05）。
    #[test]
    fn a_dashboard_of_another_organization_is_not_found() {
        let store = MemoryStore::new();
        let owner = scope_of("org-a");
        let other = scope_of("org-b");
        save_dashboard(
            &owner,
            &store,
            sample_dashboard(vec![sample_widget("w1", "wf-1")]),
        )
        .unwrap();

        assert!(list_dashboards(&other, &store).unwrap().is_empty());
        assert!(matches!(
            get_dashboard(&other, &store, "dash-1"),
            Err(DashboardStoreError::NotFound(_))
        ));
    }

    /// 别的组织的 widget 同样读不到。
    #[test]
    fn the_widgets_of_another_organization_are_not_found() {
        let store = MemoryStore::new();
        let owner = scope_of("org-a");
        let other = scope_of("org-b");
        save_dashboard(
            &owner,
            &store,
            sample_dashboard(vec![sample_widget("w1", "wf-1")]),
        )
        .unwrap();

        assert!(store.list_widget_rows(&other, "dash-1").unwrap().is_empty());
        assert!(matches!(
            store.widget_row(&other, "w1"),
            Err(DashboardStoreError::NotFound(_))
        ));
    }

    /// 同一个存储实现上，两个组织各自看到自己的仪表板。
    #[test]
    fn two_organizations_keep_their_own_dashboards_apart() {
        let store = MemoryStore::new();
        let a = scope_of("org-a");
        let b = scope_of("org-b");
        save_dashboard(
            &a,
            &store,
            sample_dashboard_with("dash-1", "A Board", vec![]),
        )
        .unwrap();
        save_dashboard(
            &b,
            &store,
            sample_dashboard_with("dash-1", "B Board", vec![]),
        )
        .unwrap();

        let list_a = list_dashboards(&a, &store).unwrap();
        let list_b = list_dashboards(&b, &store).unwrap();
        assert_eq!(list_a.len(), 1);
        assert_eq!(list_b.len(), 1);
        assert_eq!(list_a[0].name, "A Board");
        assert_eq!(list_b[0].name, "B Board");
    }

    /// 别的组织既改不动也删不掉本组织的仪表板。
    #[test]
    fn a_dashboard_of_another_organization_cannot_be_changed_or_removed() {
        let store = MemoryStore::new();
        let owner = scope_of("org-a");
        let other = scope_of("org-b");
        save_dashboard(&owner, &store, sample_dashboard(vec![])).unwrap();

        assert!(matches!(
            set_dashboard_refresh_paused(&other, &store, "dash-1", true),
            Err(DashboardStoreError::NotFound(_))
        ));
        delete_dashboard(&other, &store, "dash-1").unwrap();
        assert!(
            get_dashboard(&owner, &store, "dash-1").is_ok(),
            "别的组织删除不得影响本组织的仪表板"
        );
    }
}
