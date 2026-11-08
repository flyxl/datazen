//! Dashboard 运行时五表的服务端 SQLite 适配器（W8 刀3d-1；契约 §6.15 的
//! 存储半边）。
//!
//! 行词汇（[`DashboardRecord`] / [`WidgetRecord`] / [`WidgetRunRecord`]）
//! 与登记规则（保存归一化/组合投影/运行转换）唯一定义在共享层
//! `datazen::dashboard`（`domain::repository` / `application::runs`）；本件
//! 只负责 v14 五表的行存取与隔离边界：
//!
//! - **组织级隔离**（CM-05）：所有查询按 `organization_id` 限定，越组织
//!   读取落空（`NotFound` / 空列表）、写只落本组织。桌面是单租户（scope
//!   判定在代码层），服务端把组织键提进行——v11 查询资产 / v13 workflow
//!   同款裁决。
//! - **`DashboardRepository` 的服务端实现**：trait 归共享层
//!   `dashboard::domain::repository`。共享层 `save_dashboard` 的全部登记
//!   规则（refresh 归一化、消失 widget 清理、created_at 保留）直接架在
//!   本件之上，不复制。
//! - **`DashboardRunRepository` 的服务端实现**：运行行写入 + 保留策略
//!   （count 截断保最新 / days 过期清理 / latest 伴表防误删最新一条——
//!   桌面 `AppDb::prune_runs` 同款语义）；`MAX_RUN_ROWS` JSON 载荷截断
//!   与共享层常量一致。
//! - **monitor 授权单存储**：整档 JSON（`MonitorGrant` 自序列化），按
//!   widget 取最新一条（rowid 序，桌面 `widget_monitor_grant` 同口径）。
//!   授权/撤销的业务闸门不在本件（域路由与调度宿主各持其半）。
//! - **异步边界**：与 `storage::workflows` 同款——同步核心 +
//!   `spawn_blocking` 包装。

use std::sync::Arc;

use rusqlite::{params, Connection, OptionalExtension};

use datazen::dashboard::records::{DashboardRecord, WidgetRecord, WidgetRunRecord};
use datazen::dashboard::repository::DashboardRepository;
use datazen::dashboard::repository::DashboardRepositoryError;
use datazen::dashboard::runs::{DashboardRunRepository, DashboardRunsError};
use datazen::dashboard::types::DashboardWorkflowRef;
use datazen::monitor::domain::MonitorGrant;
use datazen::shared::scope::OrganizationScope;

use crate::storage::AdminStore;

/// 存储面错误（路由读面 → 503 / 400 映射在域路由）。
#[derive(Debug, thiserror::Error)]
pub enum DashboardStoreError {
    #[error("dashboard store: {0}")]
    Backend(String),
}

fn backend(error: impl std::fmt::Display) -> DashboardStoreError {
    DashboardStoreError::Backend(error.to_string())
}

fn repo_db(error: impl std::fmt::Display) -> DashboardRepositoryError {
    DashboardRepositoryError::Db(error.to_string())
}

// ── 仪表板与组件（DashboardRepository 的服务端实现） ────────────────────────

/// v14 `dashboards` / `widgets` 双表的存取（组织作用域由构造参数携带）。
#[derive(Clone)]
pub struct ServerDashboardStore {
    store: Arc<AdminStore>,
    organization_id: String,
}

impl ServerDashboardStore {
    /// 按组织边界构造（HTTP handler 按认证链事实逐请求装配；调度器按
    /// 服务配置的组织装配）。
    pub fn scoped(store: Arc<AdminStore>, organization_id: &str) -> Self {
        Self {
            store,
            organization_id: organization_id.to_owned(),
        }
    }

    /// 本边界的组织作用域（共享层 repository 函数的调用参数）。
    pub fn scope(&self) -> OrganizationScope {
        OrganizationScope::new(datazen::shared::id::OrganizationId::new(
            self.organization_id.clone(),
        ))
    }

    fn with_conn<T>(
        &self,
        body: impl FnOnce(&Connection) -> Result<T, DashboardRepositoryError>,
    ) -> Result<T, DashboardRepositoryError> {
        let conn = self
            .store
            .lock_for_adapter()
            .map_err(|error| DashboardRepositoryError::Db(error.to_string()))?;
        body(&conn)
    }
}

impl DashboardRepository for ServerDashboardStore {
    fn list_dashboard_rows(
        &self,
        scope: &OrganizationScope,
    ) -> Result<Vec<DashboardRecord>, DashboardRepositoryError> {
        let rows = self.with_conn(|conn| {
            let mut statement = conn
                .prepare(
                    "SELECT id, name, created_at, updated_at, layout_cols,
                                layout_row_height, enabled, refresh_paused
                         FROM dashboards
                         WHERE organization_id = ?1
                         ORDER BY name COLLATE NOCASE",
                )
                .map_err(repo_db)?;
            let iter = statement
                .query_map(params![scope.organization_id().as_str()], map_dashboard_row)
                .map_err(repo_db)?;
            let mut rows = Vec::new();
            for row in iter {
                rows.push(row.map_err(|error| repo_db(error.to_string()))?);
            }
            Ok(rows)
        })?;
        Ok(rows)
    }

    fn dashboard_row(
        &self,
        scope: &OrganizationScope,
        id: &str,
    ) -> Result<DashboardRecord, DashboardRepositoryError> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT id, name, created_at, updated_at, layout_cols,
                        layout_row_height, enabled, refresh_paused
                 FROM dashboards
                 WHERE organization_id = ?1 AND id = ?2",
                params![scope.organization_id().as_str(), id],
                map_dashboard_row,
            )
            .map_err(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => {
                    DashboardRepositoryError::NotFound(id.to_owned())
                }
                other => repo_db(other.to_string()),
            })
        })
    }

    fn upsert_dashboard_row(
        &self,
        scope: &OrganizationScope,
        row: &DashboardRecord,
    ) -> Result<(), DashboardRepositoryError> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO dashboards (
                    organization_id, id, name, created_at, updated_at,
                    layout_cols, layout_row_height, enabled, refresh_paused
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT(organization_id, id) DO UPDATE SET
                    name = excluded.name,
                    updated_at = excluded.updated_at,
                    layout_cols = excluded.layout_cols,
                    layout_row_height = excluded.layout_row_height,
                    enabled = excluded.enabled,
                    refresh_paused = excluded.refresh_paused",
                params![
                    scope.organization_id().as_str(),
                    row.id,
                    row.name,
                    row.created_at,
                    row.updated_at,
                    row.layout_cols as i64,
                    row.layout_row_height as i64,
                    row.enabled as i32,
                    row.refresh_paused as i32,
                ],
            )
            .map_err(repo_db)?;
            Ok(())
        })
    }

    fn delete_dashboard_row(
        &self,
        scope: &OrganizationScope,
        id: &str,
    ) -> Result<(), DashboardRepositoryError> {
        self.with_conn(|conn| {
            // 级联语义（桌面 SQLite `ON DELETE CASCADE` 的服务端对偶）：
            // 组件与运行行随仪表板消亡；组件的运行行先清（widgets 的
            // 外键不在本表形上，删除顺序由本件编排）。
            let widget_ids: Vec<String> = {
                let mut statement = conn
                    .prepare(
                        "SELECT id FROM widgets WHERE organization_id = ?1 AND dashboard_id = ?2",
                    )
                    .map_err(repo_db)?;
                let iter = statement
                    .query_map(params![scope.organization_id().as_str(), id], |row| {
                        row.get(0)
                    })
                    .map_err(repo_db)?;
                let mut ids = Vec::new();
                for row in iter {
                    ids.push(row.map_err(|error| repo_db(error.to_string()))?);
                }
                ids
            };
            for widget_id in widget_ids {
                conn.execute(
                    "DELETE FROM widget_latest_run WHERE organization_id = ?1 AND widget_id = ?2",
                    params![scope.organization_id().as_str(), widget_id],
                )
                .map_err(repo_db)?;
                conn.execute(
                    "DELETE FROM widget_runs WHERE organization_id = ?1 AND widget_id = ?2",
                    params![scope.organization_id().as_str(), widget_id],
                )
                .map_err(repo_db)?;
            }
            conn.execute(
                "DELETE FROM widgets WHERE organization_id = ?1 AND dashboard_id = ?2",
                params![scope.organization_id().as_str(), id],
            )
            .map_err(repo_db)?;
            let removed = conn
                .execute(
                    "DELETE FROM dashboards WHERE organization_id = ?1 AND id = ?2",
                    params![scope.organization_id().as_str(), id],
                )
                .map_err(repo_db)?;
            if removed == 0 {
                return Err(DashboardRepositoryError::NotFound(id.to_owned()));
            }
            Ok(())
        })
    }

    fn set_dashboard_refresh_paused(
        &self,
        scope: &OrganizationScope,
        id: &str,
        paused: bool,
    ) -> Result<(), DashboardRepositoryError> {
        self.with_conn(|conn| {
            let updated = conn
                .execute(
                    "UPDATE dashboards
                     SET refresh_paused = ?1, updated_at = ?2
                     WHERE organization_id = ?3 AND id = ?4",
                    params![
                        paused as i32,
                        chrono::Utc::now().to_rfc3339(),
                        scope.organization_id().as_str(),
                        id,
                    ],
                )
                .map_err(repo_db)?;
            if updated == 0 {
                return Err(DashboardRepositoryError::NotFound(id.to_owned()));
            }
            Ok(())
        })
    }

    fn list_widget_rows(
        &self,
        scope: &OrganizationScope,
        dashboard_id: &str,
    ) -> Result<Vec<WidgetRecord>, DashboardRepositoryError> {
        let rows = self.with_conn(|conn| {
            let mut statement = conn
                .prepare(
                    "SELECT id, dashboard_id, title, workflow_id, view_mode,
                                chart_config_json, layout_x, layout_y, layout_w, layout_h,
                                refresh_mode, refresh_sec, alert_json, enabled, sort_order,
                                created_at, updated_at
                         FROM widgets
                         WHERE organization_id = ?1 AND dashboard_id = ?2
                         ORDER BY sort_order, title",
                )
                .map_err(repo_db)?;
            let iter = statement
                .query_map(
                    params![scope.organization_id().as_str(), dashboard_id],
                    map_widget_row,
                )
                .map_err(repo_db)?;
            let mut rows = Vec::new();
            for row in iter {
                rows.push(row.map_err(|error| repo_db(error.to_string()))?);
            }
            Ok(rows)
        })?;
        Ok(rows)
    }

    fn widget_row(
        &self,
        scope: &OrganizationScope,
        id: &str,
    ) -> Result<WidgetRecord, DashboardRepositoryError> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT id, dashboard_id, title, workflow_id, view_mode,
                        chart_config_json, layout_x, layout_y, layout_w, layout_h,
                        refresh_mode, refresh_sec, alert_json, enabled, sort_order,
                        created_at, updated_at
                 FROM widgets
                 WHERE organization_id = ?1 AND id = ?2",
                params![scope.organization_id().as_str(), id],
                map_widget_row,
            )
            .map_err(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => {
                    DashboardRepositoryError::NotFound(id.to_owned())
                }
                other => repo_db(other.to_string()),
            })
        })
    }

    fn upsert_widget_row(
        &self,
        scope: &OrganizationScope,
        row: &WidgetRecord,
    ) -> Result<(), DashboardRepositoryError> {
        // 行级校验单家在共享层（W8 刀3d 下沉；桌面同源）。
        datazen::dashboard::repository::validate_view_mode(&row.view_mode)
            .map_err(|error| DashboardRepositoryError::Parse(error.to_string()))?;
        datazen::dashboard::repository::validate_refresh_pair(
            Some(&row.refresh_mode),
            row.refresh_sec,
        )
        .map_err(|error| DashboardRepositoryError::Parse(error.to_string()))?;
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO widgets (
                    organization_id, id, dashboard_id, title, workflow_id, view_mode,
                    chart_config_json, layout_x, layout_y, layout_w, layout_h,
                    refresh_mode, refresh_sec, alert_json, enabled, sort_order,
                    created_at, updated_at
                 ) VALUES (
                    ?1, ?2, ?3, ?4, ?5, ?6,
                    ?7, ?8, ?9, ?10, ?11,
                    ?12, ?13, ?14, ?15, ?16,
                    ?17, ?18
                 )
                 ON CONFLICT(organization_id, id) DO UPDATE SET
                    dashboard_id = excluded.dashboard_id,
                    title = excluded.title,
                    workflow_id = excluded.workflow_id,
                    view_mode = excluded.view_mode,
                    chart_config_json = excluded.chart_config_json,
                    layout_x = excluded.layout_x,
                    layout_y = excluded.layout_y,
                    layout_w = excluded.layout_w,
                    layout_h = excluded.layout_h,
                    refresh_mode = excluded.refresh_mode,
                    refresh_sec = excluded.refresh_sec,
                    alert_json = excluded.alert_json,
                    enabled = excluded.enabled,
                    sort_order = excluded.sort_order,
                    updated_at = excluded.updated_at",
                params![
                    scope.organization_id().as_str(),
                    row.id,
                    row.dashboard_id,
                    row.title,
                    row.workflow_id,
                    row.view_mode,
                    row.chart_config_json,
                    row.layout_x as i64,
                    row.layout_y as i64,
                    row.layout_w as i64,
                    row.layout_h as i64,
                    row.refresh_mode,
                    row.refresh_sec.map(|value| value as i64),
                    row.alert_json,
                    row.enabled as i32,
                    row.sort_order,
                    row.created_at,
                    row.updated_at,
                ],
            )
            .map_err(repo_db)?;
            Ok(())
        })
    }

    fn delete_widget_row(
        &self,
        scope: &OrganizationScope,
        id: &str,
    ) -> Result<(), DashboardRepositoryError> {
        self.with_conn(|conn| {
            conn.execute(
                "DELETE FROM widget_latest_run WHERE organization_id = ?1 AND widget_id = ?2",
                params![scope.organization_id().as_str(), id],
            )
            .map_err(repo_db)?;
            conn.execute(
                "DELETE FROM widget_runs WHERE organization_id = ?1 AND widget_id = ?2",
                params![scope.organization_id().as_str(), id],
            )
            .map_err(repo_db)?;
            let removed = conn
                .execute(
                    "DELETE FROM widgets WHERE organization_id = ?1 AND id = ?2",
                    params![scope.organization_id().as_str(), id],
                )
                .map_err(repo_db)?;
            if removed == 0 {
                return Err(DashboardRepositoryError::NotFound(id.to_owned()));
            }
            Ok(())
        })
    }

    fn find_workflow_refs(
        &self,
        scope: &OrganizationScope,
        workflow_id: &str,
    ) -> Result<Vec<DashboardWorkflowRef>, DashboardRepositoryError> {
        let rows = self.with_conn(|conn| {
            let mut statement = conn
                .prepare(
                    "SELECT w.workflow_id, w.dashboard_id, w.id, d.name, w.title
                         FROM widgets w
                         JOIN dashboards d
                           ON d.organization_id = w.organization_id AND d.id = w.dashboard_id
                         WHERE w.organization_id = ?1 AND w.workflow_id = ?2
                         ORDER BY d.name COLLATE NOCASE, w.title",
                )
                .map_err(repo_db)?;
            let iter = statement
                .query_map(
                    params![scope.organization_id().as_str(), workflow_id],
                    |row| {
                        Ok(DashboardWorkflowRef {
                            workflow_id: row.get(0)?,
                            dashboard_id: row.get(1)?,
                            widget_id: row.get(2)?,
                            dashboard_name: row.get(3)?,
                            widget_title: row.get(4)?,
                        })
                    },
                )
                .map_err(repo_db)?;
            let mut rows = Vec::new();
            for row in iter {
                rows.push(row.map_err(|error| repo_db(error.to_string()))?);
            }
            Ok(rows)
        })?;
        Ok(rows)
    }
}

// ── 运行历史（DashboardRunRepository 的服务端实现） ──────────────────────────

/// v14 `widget_runs` / `widget_latest_run` 双表的存取（组织作用域随每次
/// 调用的 `scope` 参数——运行行按 widget 归组织，无跨行作用域可缓存）。
#[derive(Clone)]
pub struct ServerDashboardRuns {
    store: Arc<AdminStore>,
}

impl ServerDashboardRuns {
    /// 构造（`scoped` 命名与存储家族一致；组织边界在调用参数上）。
    pub fn scoped(store: Arc<AdminStore>, _organization_id: &str) -> Self {
        Self { store }
    }

    fn with_conn<T>(
        &self,
        body: impl FnOnce(&Connection) -> Result<T, DashboardRunsError>,
    ) -> Result<T, DashboardRunsError> {
        let conn = self
            .store
            .lock_for_adapter()
            .map_err(|error| DashboardRunsError::Db(error.to_string()))?;
        body(&conn)
    }
}

fn runs_backend(error: impl std::fmt::Display) -> DashboardRunsError {
    DashboardRunsError::Db(error.to_string())
}

impl DashboardRunRepository for ServerDashboardRuns {
    fn write_run_row(
        &self,
        scope: &OrganizationScope,
        mut run: WidgetRunRecord,
        retention_count: u32,
        retention_days: u32,
    ) -> Result<(), DashboardRunsError> {
        // JSON 载荷截断（桌面 `AppDb::write_run` 同款：单行运行不携带
        // 超过 `MAX_RUN_ROWS` 的数据行）。
        if let Ok(mut rows) = serde_json::from_str::<Vec<serde_json::Value>>(&run.rows_json) {
            if rows.len() > datazen::dashboard::execute::MAX_RUN_ROWS {
                rows.truncate(datazen::dashboard::execute::MAX_RUN_ROWS);
                run.rows_json = serde_json::to_string(&rows).unwrap_or_else(|_| "[]".into());
                run.row_count = run
                    .row_count
                    .min(datazen::dashboard::execute::MAX_RUN_ROWS as u32);
            }
        }
        self.with_conn(|conn| {
                conn.execute(
                    "INSERT INTO widget_runs (
                        organization_id, id, dashboard_id, widget_id, workflow_id,
                        started_at, finished_at, status, error, row_count,
                        columns_json, rows_json, variables_json, alert_fired, alert_value
                     ) VALUES (
                        ?1, ?2, ?3, ?4, ?5,
                        ?6, ?7, ?8, ?9, ?10,
                        ?11, ?12, ?13, ?14, ?15
                     )",
                    params![
                        scope.organization_id().as_str(),
                        run.id,
                        run.dashboard_id,
                        run.widget_id,
                        run.workflow_id,
                        run.started_at,
                        run.finished_at,
                        run.status,
                        run.error,
                        run.row_count as i64,
                        run.columns_json,
                        run.rows_json,
                        run.variables_json,
                        run.alert_fired.map(|value| value as i32),
                        run.alert_value,
                    ],
                )
                .map_err(runs_backend)?;
                conn.execute(
                    "INSERT INTO widget_latest_run (organization_id, widget_id, run_id, started_at, status)
                     VALUES (?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT(organization_id, widget_id) DO UPDATE SET
                        run_id = excluded.run_id,
                        started_at = excluded.started_at,
                        status = excluded.status",
                    params![
                        scope.organization_id().as_str(),
                        run.widget_id,
                        run.id,
                        run.started_at,
                        run.status,
                    ],
                )
                .map_err(runs_backend)?;
                // 保留策略（桌面 `prune_runs` 同语义）：
                // ① count 截断（保最新 N 条）；② days 过期（latest 伴表
                // 保住最新一条不被误删）。
                if retention_count > 0 {
                    conn.execute(
                        "DELETE FROM widget_runs
                         WHERE organization_id = ?1 AND widget_id = ?2
                           AND id NOT IN (
                             SELECT id FROM (
                               SELECT id FROM widget_runs
                               WHERE organization_id = ?1 AND widget_id = ?2
                               ORDER BY started_at DESC
                               LIMIT ?3
                             )
                           )",
                        params![
                            scope.organization_id().as_str(),
                            run.widget_id,
                            retention_count as i64,
                        ],
                    )
                    .map_err(runs_backend)?;
                }
                if retention_days > 0 {
                    let cutoff = (chrono::Utc::now()
                        - chrono::Duration::days(retention_days as i64))
                    .to_rfc3339();
                    conn.execute(
                        "DELETE FROM widget_runs
                         WHERE organization_id = ?1 AND widget_id = ?2
                           AND started_at < ?3
                           AND id NOT IN (
                             SELECT run_id FROM widget_latest_run
                             WHERE organization_id = ?1 AND widget_id = ?2
                           )",
                        params![scope.organization_id().as_str(), run.widget_id, cutoff],
                    )
                    .map_err(runs_backend)?;
                }
                Ok(())
            })
    }

    fn list_run_rows(
        &self,
        scope: &OrganizationScope,
        widget_id: &str,
        limit: u32,
    ) -> Result<Vec<WidgetRunRecord>, DashboardRunsError> {
        let rows = self.with_conn(|conn| {
            let mut statement = conn
                .prepare(
                    "SELECT id, dashboard_id, widget_id, workflow_id, started_at,
                                finished_at, status, error, row_count, columns_json,
                                rows_json, variables_json, alert_fired, alert_value
                         FROM widget_runs
                         WHERE organization_id = ?1 AND widget_id = ?2
                         ORDER BY started_at DESC
                         LIMIT ?3",
                )
                .map_err(runs_backend)?;
            let iter = statement
                .query_map(
                    params![scope.organization_id().as_str(), widget_id, limit as i64],
                    map_run_row,
                )
                .map_err(runs_backend)?;
            let mut rows = Vec::new();
            for row in iter {
                rows.push(row.map_err(|error| runs_backend(error.to_string()))?);
            }
            Ok(rows)
        })?;
        Ok(rows)
    }

    fn run_row(
        &self,
        scope: &OrganizationScope,
        run_id: &str,
    ) -> Result<WidgetRunRecord, DashboardRunsError> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT id, dashboard_id, widget_id, workflow_id, started_at,
                        finished_at, status, error, row_count, columns_json,
                        rows_json, variables_json, alert_fired, alert_value
                 FROM widget_runs
                 WHERE organization_id = ?1 AND id = ?2",
                params![scope.organization_id().as_str(), run_id],
                map_run_row,
            )
            .map_err(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => {
                    DashboardRunsError::NotFound(run_id.to_owned())
                }
                other => runs_backend(other.to_string()),
            })
        })
    }
}

// ── monitor 授权单（整档 JSON 存取） ────────────────────────────────────────

/// v14 `monitor_schedule_grants` 表的存取（组织作用域由构造参数携带）。
#[derive(Clone)]
pub struct ServerMonitorGrants {
    store: Arc<AdminStore>,
    organization_id: String,
}

impl ServerMonitorGrants {
    /// 按组织边界构造。
    pub fn scoped(store: Arc<AdminStore>, organization_id: &str) -> Self {
        Self {
            store,
            organization_id: organization_id.to_owned(),
        }
    }

    /// 本边界的组织作用域。
    pub fn scope(&self) -> OrganizationScope {
        OrganizationScope::new(datazen::shared::id::OrganizationId::new(
            self.organization_id.clone(),
        ))
    }

    /// 按 id 取一条授权单（越组织 = None）。
    pub fn get(&self, id: &str) -> Result<Option<MonitorGrant>, DashboardStoreError> {
        self.with_conn(|conn| {
            let json: Option<String> = conn
                .query_row(
                    "SELECT metadata_json FROM monitor_schedule_grants
                     WHERE organization_id = ?1 AND id = ?2",
                    params![self.organization_id, id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(backend)?;
            json.map(|json| serde_json::from_str(&json))
                .transpose()
                .map_err(|_| backend("invalid monitor grant metadata"))
        })
    }

    /// 取某 widget 的最新授权单（rowid 序，桌面 `widget_monitor_grant`
    /// 同口径；越组织 = None）。
    pub fn for_widget(
        &self,
        dashboard_id: &str,
        widget_id: &str,
    ) -> Result<Option<MonitorGrant>, DashboardStoreError> {
        self.with_conn(|conn| {
            let json: Option<String> = conn
                .query_row(
                    "SELECT metadata_json FROM monitor_schedule_grants
                     WHERE organization_id = ?1 AND dashboard_id = ?2 AND widget_id = ?3
                     ORDER BY rowid DESC
                     LIMIT 1",
                    params![self.organization_id, dashboard_id, widget_id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(backend)?;
            json.map(|json| serde_json::from_str(&json))
                .transpose()
                .map_err(|_| backend("invalid monitor grant metadata"))
        })
    }

    /// 把授权单物化成服务身份委托行（`execution_service_identity`，v7 表
    /// ——monitor 调度执行的服务主体授权机制：授权单批准的连接各一行，
    /// scope = `connection`[Connect+Query]、目标绑定该连接、过期随授权单
    /// TTL 对齐）。
    ///
    /// 行键 `(organization_id, delegation_id)` 与唯一活跃索引
    /// `(organization_id, service_principal) WHERE revoked=0` 决定了两个
    /// 命名裁决：
    /// - `delegation_id = "{grant_id}:{connection_id}"`——一条授权单多个
    ///   目标连接时各走各行（`authorize_target` 的委托分支按行内核对
    ///   目标连接）；
    /// - `service_principal = "monitor-service:{grant_id}:{connection_id}"`
    ///   ——唯一索引要求行行不同；执行上下文的主体仍是授权单的
    ///   `service_principal`（`resolve_delegation` 以上下文事实回填
    ///   DelegationRef，行内列不参与主体核对）。
    ///
    /// 幂等：重授权（同 id 新 TTL）走 ON CONFLICT 更新。
    pub fn materialize_service_identity(
        &self,
        grant: &MonitorGrant,
    ) -> Result<(), DashboardStoreError> {
        let expires_at = grant
            .expires_at
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let mut connections: Vec<&str> = grant
            .scopes
            .iter()
            .map(|scope| scope.target.connection_id.as_str())
            .collect();
        connections.sort_unstable();
        connections.dedup();
        self.with_conn(|conn| {
            for connection in connections {
                conn.execute(
                    "INSERT INTO execution_service_identity
                        (organization_id, delegation_id, service_principal, scope,
                         target_connection_id, expires_at, revoked, approved_by,
                         created_at, updated_at)
                     VALUES (?1, ?2, ?3, 'connection', ?4, ?5, 0, ?6, ?7, ?7)
                     ON CONFLICT(organization_id, delegation_id) DO UPDATE SET
                        expires_at = excluded.expires_at,
                        revoked = 0,
                        approved_by = excluded.approved_by,
                        updated_at = excluded.updated_at",
                    rusqlite::params![
                        self.organization_id,
                        format!("{}:{}", grant.id, connection),
                        format!("{}:{}", grant.service_principal, connection),
                        connection,
                        expires_at,
                        grant.approved_by,
                        now,
                    ],
                )
                .map_err(backend)?;
            }
            Ok(())
        })
    }

    /// 撤销授权单连带的服务身份委托行（前缀 `"{grant_id}:"`——授权单 id
    /// 是服务端铸造的 uuid，无 LIKE 元字符；best-effort 卫生操作，过期
    /// TTL 是兜底）。
    pub fn revoke_service_identity(&self, grant_id: &str) -> Result<(), DashboardStoreError> {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE execution_service_identity
                 SET revoked = 1, updated_at = ?3
                 WHERE organization_id = ?1 AND delegation_id LIKE ?2",
                rusqlite::params![
                    self.organization_id,
                    format!("{grant_id}:%"),
                    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
                ],
            )
            .map_err(backend)?;
            Ok(())
        })
    }

    /// 保存（插入或更新）一条授权单。
    pub fn save(&self, grant: &MonitorGrant) -> Result<(), DashboardStoreError> {
        let json =
            serde_json::to_string(grant).map_err(|_| backend("invalid monitor grant metadata"))?;
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO monitor_schedule_grants
                    (organization_id, id, dashboard_id, widget_id, metadata_json)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(organization_id, id) DO UPDATE SET
                    metadata_json = excluded.metadata_json",
                params![
                    self.organization_id,
                    grant.id,
                    grant.dashboard_id,
                    grant.widget_id,
                    json,
                ],
            )
            .map_err(backend)?;
            Ok(())
        })
    }

    fn with_conn<T>(
        &self,
        body: impl FnOnce(&Connection) -> Result<T, DashboardStoreError>,
    ) -> Result<T, DashboardStoreError> {
        let conn = self.store.lock_for_adapter().map_err(backend)?;
        body(&conn)
    }
}

// ── 行映射 ──────────────────────────────────────────────────────────────────

fn map_dashboard_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<DashboardRecord> {
    Ok(DashboardRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        created_at: row.get(2)?,
        updated_at: row.get(3)?,
        layout_cols: row.get::<_, i64>(4)? as u32,
        layout_row_height: row.get::<_, i64>(5)? as u32,
        enabled: row.get::<_, i32>(6)? != 0,
        refresh_paused: row.get::<_, i32>(7)? != 0,
    })
}

fn map_widget_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<WidgetRecord> {
    Ok(WidgetRecord {
        id: row.get(0)?,
        dashboard_id: row.get(1)?,
        title: row.get(2)?,
        workflow_id: row.get(3)?,
        view_mode: row.get(4)?,
        chart_config_json: row.get(5)?,
        layout_x: row.get::<_, i64>(6)? as u32,
        layout_y: row.get::<_, i64>(7)? as u32,
        layout_w: row.get::<_, i64>(8)? as u32,
        layout_h: row.get::<_, i64>(9)? as u32,
        refresh_mode: row.get(10)?,
        refresh_sec: row.get::<_, Option<i64>>(11)?.map(|value| value as u32),
        alert_json: row.get(12)?,
        enabled: row.get::<_, i32>(13)? != 0,
        sort_order: row.get(14)?,
        created_at: row.get(15)?,
        updated_at: row.get(16)?,
    })
}

fn map_run_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<WidgetRunRecord> {
    Ok(WidgetRunRecord {
        id: row.get(0)?,
        dashboard_id: row.get(1)?,
        widget_id: row.get(2)?,
        workflow_id: row.get(3)?,
        started_at: row.get(4)?,
        finished_at: row.get(5)?,
        status: row.get(6)?,
        error: row.get(7)?,
        row_count: row.get::<_, i64>(8)? as u32,
        columns_json: row.get(9)?,
        rows_json: row.get(10)?,
        variables_json: row.get(11)?,
        alert_fired: row.get::<_, Option<i32>>(12)?.map(|value| value != 0),
        alert_value: row.get(13)?,
    })
}
