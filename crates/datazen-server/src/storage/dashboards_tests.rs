//! v14 dashboard 五表的存储测试（W8 刀3d-1）。
//!
//! 覆盖面：`DashboardRepository` 九面（共享层 `save_and_fetch` 的登记
//! 规则架在本件之上的往返 + 全面组织隔离）、`DashboardRunRepository`
//! 三面（写入/列表/读取 + 保留策略 count 截断与 days 过期 + latest 伴表
//! 防误删）、monitor 授权单存取（最新一条口径 + 组织隔离 + 更新冲突）、
//! 下沉校验（view_mode 词表 / refresh 配对）的服务端锚钉。

use std::sync::Arc;

use datazen::dashboard::records::WidgetRunRecord;
use datazen::dashboard::repository::{
    delete_dashboard, find_workflow_refs, get_dashboard, list_dashboards, save_and_fetch,
    set_dashboard_refresh_paused, DashboardRepository,
};
use datazen::dashboard::runs::{get_run, list_run_index, write_run, DashboardRunRepository};
use datazen::dashboard::types::{
    Dashboard, DashboardLayout, DashboardWidget, MonitorSettings, RefreshMode, RefreshPolicy,
    ViewMode, WidgetLayout, WidgetRunStatus,
};
use datazen::execution::target::{ExecutionTarget, NamespaceTarget};
use datazen::monitor::domain::{MonitorGrant, MonitorScope};
use datazen::shared::id::{ConnectionId, OrganizationId};
use datazen::shared::scope::OrganizationScope;

use crate::storage::{AdminStore, ServerDashboardRuns, ServerDashboardStore, ServerMonitorGrants};

const ORG: &str = "org-dashboard-tests";
const OTHER_ORG: &str = "org-other";

fn store() -> Arc<AdminStore> {
    let store = AdminStore::open_in_memory().expect("admin store");
    store.migrate().expect("migrations");
    Arc::new(store)
}

fn scope(org: &str) -> OrganizationScope {
    OrganizationScope::new(OrganizationId::new(org))
}

fn widget(id: &str, workflow_id: &str, refresh: RefreshPolicy) -> DashboardWidget {
    DashboardWidget {
        id: id.to_owned(),
        title: format!("Widget {id}"),
        workflow_id: workflow_id.to_owned(),
        view_mode: ViewMode::Chart,
        chart_config: None,
        layout: WidgetLayout {
            x: 0,
            y: 0,
            w: 4,
            h: 3,
        },
        refresh,
        alert: None,
        enabled: true,
    }
}

fn dashboard(id: &str, widgets: Vec<DashboardWidget>) -> Dashboard {
    Dashboard {
        id: id.to_owned(),
        name: format!("Dashboard {id}"),
        created_at: "2026-01-01T00:00:00.000Z".into(),
        updated_at: "2026-01-01T00:00:00.000Z".into(),
        layout: DashboardLayout {
            cols: 12,
            row_height: 80,
        },
        widgets,
        enabled: true,
        refresh_paused: false,
    }
}

fn manual_refresh() -> RefreshPolicy {
    RefreshPolicy {
        mode: RefreshMode::Manual,
        refresh_sec: None,
    }
}

fn run_record(id: &str, widget_id: &str, started_at: &str) -> WidgetRunRecord {
    WidgetRunRecord {
        id: id.to_owned(),
        dashboard_id: "dash-1".into(),
        widget_id: widget_id.to_owned(),
        workflow_id: "wf-1".into(),
        started_at: started_at.to_owned(),
        finished_at: format!("{started_at}Z"),
        status: "ok".into(),
        error: None,
        row_count: 1,
        columns_json: "[\"v\"]".into(),
        rows_json: "[[1]]".into(),
        variables_json: None,
        alert_fired: None,
        alert_value: None,
    }
}

fn retention(count: u32, days: u32) -> MonitorSettings {
    MonitorSettings {
        tray_enabled: false,
        close_to_tray: false,
        default_webhook_url: None,
        email: None,
        max_concurrent_queries: 2,
        export_include_dashboard_runs: false,
        run_retention_count: count,
        run_retention_days: days,
    }
}

fn grant(id: &str, dashboard_id: &str, widget_id: &str, org: &str) -> MonitorGrant {
    MonitorGrant {
        id: id.to_owned(),
        dashboard_id: dashboard_id.to_owned(),
        widget_id: widget_id.to_owned(),
        workflow_id: "wf-1".into(),
        workflow_revision: "rev-1".into(),
        organization_id: org.to_owned(),
        service_principal: format!("monitor-service:{id}"),
        approved_by: "user-1".into(),
        scopes: vec![MonitorScope {
            target: ExecutionTarget::new(
                ConnectionId::new("conn-1"),
                NamespaceTarget::database("app"),
            ),
            config_revision: 1,
            command: "query".into(),
        }],
        actions: vec!["connect".into(), "query".into()],
        read_only: true,
        expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
        revocation_version: 1,
        revoked: false,
    }
}

// ── DashboardRepository ─────────────────────────────────────────────────────

#[test]
fn dashboard_save_fetch_roundtrip_applies_shared_rules() {
    let store = store();
    let dashboards = ServerDashboardStore::scoped(store.clone(), ORG);
    let org = scope(ORG);

    // 保存两个 widget（第二个 manual 带多余 refresh_sec——保存时归一化清零）。
    let second = widget(
        "w2",
        "wf-2",
        RefreshPolicy {
            mode: RefreshMode::Manual,
            refresh_sec: Some(99),
        },
    );
    let saved = save_and_fetch(
        &org,
        &dashboards,
        dashboard(
            "dash-1",
            vec![widget("w1", "wf-1", manual_refresh()), second],
        ),
    )
    .expect("save and fetch");

    // refresh 归一化：manual 模式不携带 refresh_sec。
    let stored_second = saved
        .widgets
        .iter()
        .find(|widget| widget.id == "w2")
        .expect("second widget stored");
    assert_eq!(stored_second.refresh.refresh_sec, None);
    // updated_at 服务端权威（保存时铸造）。
    assert_ne!(saved.updated_at, "2026-01-01T00:00:00.000Z");

    // 再保存：消失的 widget 被清理、留下的保留 created_at。
    let before = get_dashboard(&org, &dashboards, "dash-1")
        .expect("read back")
        .widgets[0]
        .id
        .clone();
    assert_eq!(before, "w1");
    let updated = save_and_fetch(
        &org,
        &dashboards,
        dashboard(
            "dash-1",
            vec![
                widget("w1", "wf-1", manual_refresh()),
                widget("w3", "wf-3", manual_refresh()),
            ],
        ),
    )
    .expect("second save");
    let ids: Vec<&str> = updated.widgets.iter().map(|w| w.id.as_str()).collect();
    assert_eq!(ids, vec!["w1", "w3"], "w2 was removed on save");

    // refresh_paused 半独立面。
    set_dashboard_refresh_paused(&org, &dashboards, "dash-1", true).expect("pause");
    let paused = get_dashboard(&org, &dashboards, "dash-1").expect("read");
    assert!(paused.refresh_paused);

    // 删除（未知 id → NotFound）。
    delete_dashboard(&org, &dashboards, "dash-1").expect("delete");
    assert!(get_dashboard(&org, &dashboards, "dash-1").is_err());
    assert!(delete_dashboard(&org, &dashboards, "dash-1").is_err());
}

#[test]
fn dashboards_are_isolated_per_organization_on_every_face() {
    let store = store();
    let ours = ServerDashboardStore::scoped(store.clone(), ORG);
    let theirs = ServerDashboardStore::scoped(store.clone(), OTHER_ORG);
    let our_scope = scope(ORG);
    let their_scope = scope(OTHER_ORG);

    save_and_fetch(
        &our_scope,
        &ours,
        dashboard("dash-1", vec![widget("w1", "wf-1", manual_refresh())]),
    )
    .expect("save in our org");

    // 列表 / 单读：别的组织一律「不存在」。
    assert!(list_dashboards(&their_scope, &theirs)
        .expect("list other org")
        .is_empty());
    assert!(get_dashboard(&their_scope, &theirs, "dash-1").is_err());
    assert!(ours.widget_row(&their_scope, "w1").is_err());

    // 写面：pause/delete 只作用本组织（越组织 = NotFound，不误伤）。
    assert!(set_dashboard_refresh_paused(&their_scope, &theirs, "dash-1", true).is_err());
    assert!(delete_dashboard(&their_scope, &theirs, "dash-1").is_err());
    let still_here = get_dashboard(&our_scope, &ours, "dash-1").expect("untouched");
    assert!(!still_here.refresh_paused);

    // 引用查询按组织隔离。
    let refs = find_workflow_refs(&our_scope, &ours, "wf-1").expect("refs");
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].dashboard_id, "dash-1");
    assert_eq!(refs[0].widget_id, "w1");
    assert_eq!(refs[0].widget_title, "Widget w1");
    assert!(find_workflow_refs(&their_scope, &theirs, "wf-1")
        .expect("refs other org")
        .is_empty());
}

#[test]
fn deleting_a_dashboard_cascades_widgets_and_runs() {
    let store = store();
    let dashboards = ServerDashboardStore::scoped(store.clone(), ORG);
    let runs = ServerDashboardRuns::scoped(store.clone(), ORG);
    let org = scope(ORG);

    save_and_fetch(
        &org,
        &dashboards,
        dashboard(
            "dash-1",
            vec![
                widget("w1", "wf-1", manual_refresh()),
                widget("w2", "wf-1", manual_refresh()),
            ],
        ),
    )
    .expect("save");

    write_run(
        &org,
        &runs,
        &datazen::dashboard::types::WidgetRun {
            id: "run-1".into(),
            dashboard_id: "dash-1".into(),
            widget_id: "w1".into(),
            workflow_id: "wf-1".into(),
            started_at: "2026-01-01T00:00:00".into(),
            finished_at: "2026-01-01T00:00:01".into(),
            status: WidgetRunStatus::Ok,
            error: None,
            row_count: 0,
            columns: vec![],
            rows: vec![],
            alert_fired: None,
            alert_value: None,
        },
        &retention(10, 0),
    )
    .expect("run written");

    delete_dashboard(&org, &dashboards, "dash-1").expect("delete");

    // 组件与运行行随仪表板消亡（级联 = 桌面 ON DELETE CASCADE 的对偶）。
    assert!(dashboards.widget_row(&org, "w1").is_err());
    assert!(runs.run_row(&org, "run-1").is_err());
}

#[test]
fn sunk_row_validations_are_enforced_on_widget_upsert() {
    let store = store();
    let dashboards = ServerDashboardStore::scoped(store.clone(), ORG);
    let org = scope(ORG);

    let bad_mode = widget("w1", "wf-1", manual_refresh());
    // 走行级 API 直接投坏词表（共享层 save_dashboard 会先归一化，直投
    // 仓储才能钉住校验在场）。
    let mut row = datazen::dashboard::repository::widget_to_record(&bad_mode, "dash-x", 0);
    row.view_mode = "pie".into();
    let error = dashboards
        .upsert_widget_row(&org, &row)
        .expect_err("invalid view mode rejected");
    assert!(error.to_string().contains("invalid view_mode"));

    let mut row = datazen::dashboard::repository::widget_to_record(
        &widget(
            "w2",
            "wf-1",
            RefreshPolicy {
                mode: RefreshMode::Interval,
                refresh_sec: Some(10),
            },
        ),
        "dash-x",
        0,
    );
    row.refresh_sec = Some(10);
    let error = dashboards
        .upsert_widget_row(&org, &row)
        .expect_err("interval below floor rejected");
    assert!(error.to_string().contains("refresh_sec must be >= 30"));
}

// ── DashboardRunRepository ──────────────────────────────────────────────────

#[test]
fn run_rows_roundtrip_with_retention_and_latest_protection() {
    let store = store();
    let runs = ServerDashboardRuns::scoped(store.clone(), ORG);
    let other_runs = ServerDashboardRuns::scoped(store.clone(), OTHER_ORG);
    let org = scope(ORG);
    let other_org = scope(OTHER_ORG);

    // count 截断：retention_count = 3，写 5 条 → 只保最新 3 条。
    for index in 0..5 {
        let at = format!("2026-01-01T00:0{index}:00");
        runs.write_run_row(&org, run_record(&format!("run-{index}"), "w1", &at), 3, 0)
            .expect("write run");
    }
    let index = list_run_index(&org, &runs, "dash-1", "w1", 10).expect("index");
    assert_eq!(index.len(), 3, "count retention keeps the newest three");
    assert_eq!(index[0].id, "run-4");
    assert_eq!(index[2].id, "run-2");

    // get_run 的 widget 归属核对（别的 widget 看不见）。
    let run = get_run(&org, &runs, "dash-1", "w1", "run-4").expect("get run");
    assert_eq!(run.row_count, 1);
    assert!(get_run(&org, &runs, "dash-1", "w2", "run-4").is_err());

    // days 过期：旧时间戳清理，latest 伴表保住最新一条。
    runs.write_run_row(
        &org,
        run_record("run-old", "w2", "2020-01-01T00:00:00"),
        0,
        365,
    )
    .expect("write old run");
    runs.write_run_row(
        &org,
        run_record("run-new", "w2", "2026-01-01T00:00:00"),
        0,
        365,
    )
    .expect("write new run");
    // run-new 是 latest（刚写过），run-old 超期被清。
    let w2_index = list_run_index(&org, &runs, "dash-1", "w2", 10).expect("index");
    let ids: Vec<&str> = w2_index.iter().map(|entry| entry.id.as_str()).collect();
    assert_eq!(ids, vec!["run-new"], "expired rows go, latest stays");

    // 组织隔离：别的组织读不到本组织运行。
    assert!(list_run_index(&other_org, &other_runs, "dash-1", "w1", 10)
        .expect("index other org")
        .is_empty());
    assert!(other_runs.run_row(&other_org, "run-4").is_err());
}

// ── ServerMonitorGrants ─────────────────────────────────────────────────────

#[test]
fn monitor_grants_roundtrip_latest_and_isolation() {
    let store = store();
    let grants = ServerMonitorGrants::scoped(store.clone(), ORG);
    let other_grants = ServerMonitorGrants::scoped(store.clone(), OTHER_ORG);

    let first = grant("g-1", "dash-1", "w1", ORG);
    grants.save(&first).expect("save first");

    assert_eq!(grants.get("g-1").expect("get").expect("present").id, "g-1");
    assert_eq!(
        grants
            .for_widget("dash-1", "w1")
            .expect("for widget")
            .expect("present")
            .id,
        "g-1"
    );

    // 同 widget 第二条授权单后写 → for_widget 取最新（rowid 序）。
    let second = grant("g-2", "dash-1", "w1", ORG);
    grants.save(&second).expect("save second");
    assert_eq!(
        grants
            .for_widget("dash-1", "w1")
            .expect("for widget")
            .expect("present")
            .id,
        "g-2"
    );

    // 撤销 = 更新同 id（ON CONFLICT 更新 metadata）。
    let mut revoked = second.clone();
    revoked.revoked = true;
    revoked.revocation_version = 2;
    grants.save(&revoked).expect("save revoke");
    let stored = grants.get("g-2").expect("get").expect("present");
    assert!(stored.revoked);
    assert_eq!(stored.revocation_version, 2);

    // 组织隔离：别的组织不可见。
    assert!(other_grants.get("g-1").expect("get").is_none());
    assert!(other_grants
        .for_widget("dash-1", "w1")
        .expect("for widget")
        .is_none());
    // 保存只落本组织（OTHER_ORG 的表里查不到 g-1 的组织键行）。
    assert_eq!(
        other_grants
            .get("g-1")
            .expect("get")
            .map(|grant| grant.organization_id),
        None
    );
}
