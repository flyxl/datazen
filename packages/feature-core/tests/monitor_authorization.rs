use async_trait::async_trait;
use datazen_feature_core::dashboard::{
    monitor::{MonitorEngine, MonitorHost},
    schedule_grant::{MonitorGrant, MonitorScope},
    types::*,
};
use datazen_platform_api::{
    id::ConnectionId,
    target::{ExecutionTarget, NamespaceTarget},
};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};
use tokio::sync::{Notify, RwLock};
fn widget() -> DashboardWidget {
    serde_json::from_value(serde_json::json!({"id":"w","title":"Report","workflowId":"wf","viewMode":"table","layout":{"x":0,"y":0,"w":4,"h":3},"refresh":{"mode":"interval","refreshSec":30},"enabled":true})).unwrap()
}
fn dashboard() -> Dashboard {
    Dashboard {
        id: "d".into(),
        name: "Ops".into(),
        created_at: "".into(),
        updated_at: "".into(),
        layout: DashboardLayout {
            cols: 12,
            row_height: 80,
        },
        widgets: vec![widget()],
        enabled: true,
        refresh_paused: false,
    }
}
fn grant() -> MonitorGrant {
    MonitorGrant {
        id: "grant".into(),
        dashboard_id: "d".into(),
        widget_id: "w".into(),
        workflow_id: "wf".into(),
        workflow_revision: "fixed-revision".into(),
        organization_id: "org".into(),
        service_principal: "monitor-service:grant".into(),
        approved_by: "desktop-user".into(),
        scopes: vec![MonitorScope {
            target: ExecutionTarget::new(ConnectionId::new("pg"), NamespaceTarget::database("app")),
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
fn run() -> WidgetRun {
    WidgetRun {
        id: "r".into(),
        dashboard_id: "d".into(),
        widget_id: "w".into(),
        workflow_id: "wf".into(),
        started_at: "".into(),
        finished_at: "".into(),
        status: WidgetRunStatus::Ok,
        error: None,
        row_count: 1,
        columns: vec![],
        rows: vec![],
        alert_fired: None,
        alert_value: None,
    }
}
struct Host {
    dashboard: RwLock<Dashboard>,
    grant: RwLock<Option<MonitorGrant>>,
    manual: AtomicUsize,
    scheduled: AtomicUsize,
    hold_manual: AtomicBool,
    started: Notify,
    release: Notify,
    checked: Notify,
}
impl Host {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            dashboard: RwLock::new(dashboard()),
            grant: RwLock::new(Some(grant())),
            manual: AtomicUsize::new(0),
            scheduled: AtomicUsize::new(0),
            hold_manual: AtomicBool::new(false),
            started: Notify::new(),
            release: Notify::new(),
            checked: Notify::new(),
        })
    }
}
#[async_trait]
impl MonitorHost for Host {
    async fn dashboards(&self) -> Result<Vec<Dashboard>, String> {
        Ok(vec![self.dashboard.read().await.clone()])
    }
    async fn max_concurrent_queries(&self) -> usize {
        2
    }
    async fn execute_widget(&self, _: &str, _: &DashboardWidget) -> Result<WidgetRun, String> {
        self.manual.fetch_add(1, Ordering::SeqCst);
        if self.hold_manual.load(Ordering::SeqCst) {
            self.started.notify_one();
            self.release.notified().await;
        }
        Ok(run())
    }
    async fn scheduled_authorization(
        &self,
        _: &str,
        _: &DashboardWidget,
    ) -> Result<MonitorGrant, String> {
        let grant = self.grant.read().await.clone().ok_or("grant missing")?;
        self.checked.notify_one();
        Ok(grant)
    }
    async fn execute_scheduled_widget(
        &self,
        _: &str,
        _: &DashboardWidget,
        frozen: &MonitorGrant,
    ) -> Result<WidgetRun, String> {
        let current = self.grant.read().await.clone().ok_or("grant missing")?;
        current
            .validate(&frozen.context(), chrono::Utc::now())
            .map_err(str::to_owned)?;
        if current != *frozen {
            return Err("grant changed".into());
        }
        assert_ne!(frozen.context().principal_id.as_str(), "desktop-user");
        assert!(frozen.context().is_delegated());
        self.scheduled.fetch_add(1, Ordering::SeqCst);
        Ok(run())
    }
    async fn deliver_run(&self, _: &str, _: &DashboardWidget, _: &WidgetRun) {}
}
#[tokio::test]
async fn authorized_schedule_uses_frozen_backend_service_grant() {
    let host = Host::new();
    let engine = MonitorEngine::new(host.clone());
    assert_eq!(
        engine
            .tick_scheduled_widget("d", &widget())
            .await
            .unwrap()
            .status,
        WidgetRunStatus::Ok
    );
    assert_eq!(host.scheduled.load(Ordering::SeqCst), 1);
    assert_eq!(host.manual.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn missing_expired_revoked_and_wrong_scope_never_fall_back_to_manual() {
    for case in ["missing", "expired", "revoked", "scope", "read-only"] {
        let host = Host::new();
        {
            let mut value = host.grant.write().await;
            match case {
                "missing" => *value = None,
                "expired" => {
                    value.as_mut().unwrap().expires_at =
                        chrono::Utc::now() - chrono::Duration::seconds(1)
                }
                "revoked" => value.as_mut().unwrap().revoked = true,
                "scope" => value.as_mut().unwrap().widget_id = "other".into(),
                _ => value.as_mut().unwrap().read_only = false,
            }
        }
        let engine = MonitorEngine::new(host.clone());
        assert!(
            engine.tick_scheduled_widget("d", &widget()).await.is_err(),
            "{case}"
        );
        assert_eq!(host.scheduled.load(Ordering::SeqCst), 0);
        assert_eq!(host.manual.load(Ordering::SeqCst), 0);
        assert!(
            engine.tick_widget("d", &widget()).await.is_ok(),
            "manual remains user execution: {case}"
        );
        assert_eq!(host.manual.load(Ordering::SeqCst), 1);
    }
}
#[tokio::test]
async fn queued_tick_rechecks_disable_revocation_and_definition_revision_after_lock() {
    for change in ["disable", "revoke", "definition", "target"] {
        let host = Host::new();
        host.hold_manual.store(true, Ordering::SeqCst);
        let engine = MonitorEngine::new(host.clone());
        let manual = {
            let engine = engine.clone();
            tokio::spawn(async move { engine.tick_widget("d", &widget()).await })
        };
        host.started.notified().await;
        let scheduled = {
            let engine = engine.clone();
            tokio::spawn(async move { engine.tick_scheduled_widget("d", &widget()).await })
        };
        host.checked.notified().await;
        match change {
            "disable" => host.dashboard.write().await.enabled = false,
            "revoke" => host.grant.write().await.as_mut().unwrap().revoked = true,
            "definition" => {
                host.grant.write().await.as_mut().unwrap().workflow_revision = "new-revision".into()
            }
            _ => {
                host.grant.write().await.as_mut().unwrap().scopes[0]
                    .target
                    .namespace
                    .database = Some("other".into())
            }
        }
        host.release.notify_one();
        assert!(
            manual.await.unwrap().is_ok(),
            "in-flight manual outcome is preserved"
        );
        assert!(scheduled.await.unwrap().is_err(), "{change}");
        assert_eq!(host.scheduled.load(Ordering::SeqCst), 0);
    }
}
#[test]
fn service_grant_cannot_be_attached_to_a_user_or_another_client() {
    let grant = grant();
    let mut context = grant.context();
    assert!(grant.validate(&context, chrono::Utc::now()).is_ok());
    context.principal_id = datazen_platform_api::id::PrincipalId::new("desktop-user");
    assert!(grant.validate(&context, chrono::Utc::now()).is_err());
    context = grant.context();
    context.client_instance_id = datazen_platform_api::id::ClientInstanceId::new("another-client");
    assert!(grant.validate(&context, chrono::Utc::now()).is_err());
}

#[test]
fn scheduled_sql_accepts_queries_but_rejects_writes_hidden_in_query_shapes() {
    use datazen_feature_core::dashboard::schedule_grant::validate_read_only_sql;
    for sql in [
        "SELECT 1",
        "SELECT 'DELETE' AS keyword",
        "WITH x AS (SELECT 1 AS n) SELECT n FROM x",
        "SELECT 1 UNION ALL SELECT 2",
    ] {
        assert!(validate_read_only_sql(sql).is_ok(), "{sql}");
    }
    for sql in [
        "DELETE FROM sample",
        "SELECT 1; DELETE FROM sample",
        "SELECT * INTO new_table FROM sample",
        "SELECT * FROM sample FOR UPDATE",
        "WITH changed AS (DELETE FROM sample RETURNING *) SELECT * FROM changed",
        "EXPLAIN ANALYZE DELETE FROM sample",
    ] {
        assert!(validate_read_only_sql(sql).is_err(), "{sql}");
    }
}

#[test]
fn scheduled_read_only_queries_use_a_closed_function_policy() {
    use datazen_feature_core::dashboard::schedule_grant::validate_read_only_sql;
    for sql in [
        "SELECT count(*), sum(value), avg(value), min(value), max(value) FROM metrics",
        "SELECT COALESCE(ROUND(AVG(value), 2), 0) FROM metrics",
        "SELECT lower(name), upper(name), length(name) FROM metrics",
        "WITH totals AS (SELECT count(*) n FROM metrics) SELECT n FROM totals",
        "SELECT count(*) OVER () FROM metrics",
    ] {
        assert!(
            validate_read_only_sql(sql).is_ok(),
            "allowed query rejected: {sql}"
        );
    }
    for sql in [
        "SELECT nextval('sequence_name')",
        "SELECT setval('sequence_name', 42)",
        "SELECT currval('sequence_name')",
        "SELECT lastval()",
        "SELECT pg_catalog.nextval('sequence_name')",
        r#"SELECT "nextval"('sequence_name')"#,
        "SELECT GET_LOCK('monitor', 0)",
        "SELECT RELEASE_LOCK('monitor')",
        "SELECT RELEASE_ALL_LOCKS()",
        "SELECT IS_FREE_LOCK('monitor')",
        "SELECT IS_USED_LOCK('monitor')",
        "SELECT pg_advisory_lock(1)",
        "SELECT pg_try_advisory_lock(1)",
        "SELECT pg_advisory_unlock(1)",
        "SELECT pg_advisory_unlock_all()",
        "SELECT pg_advisory_xact_lock(1)",
        "SELECT pg_try_advisory_xact_lock_shared(1)",
        "SELECT set_config('application_name', 'changed', false)",
        "SELECT custom_write()",
        "SELECT custom.count(*) FROM metrics",
        r#"SELECT "COUNT"(*) FROM metrics"#,
        "SELECT * FROM nextval('sequence_name')",
        "SELECT count(*) FROM metrics WHERE nextval('sequence_name') > 0",
        "SELECT COALESCE((SELECT GET_LOCK('nested', 0)), 0)",
        "WITH changed AS (SELECT setval('sequence_name', 42)) SELECT * FROM changed",
        "SELECT 1 UNION ALL SELECT GET_LOCK('union', 0)",
    ] {
        assert!(
            validate_read_only_sql(sql).is_err(),
            "unapproved function accepted: {sql}"
        );
    }
}

#[test]
fn scheduled_sql_rejects_qualified_relations_at_every_query_depth() {
    use datazen_feature_core::dashboard::schedule_grant::validate_read_only_sql;
    // Qualification is refused regardless of case or quoting, including names
    // that could match the approved target. No textual namespace comparison.
    for relation in [
        "other_schema.secret",
        "otherdb.secret",
        "app.secret",
        "OTHER_SCHEMA.SECRET",
        r#""other_schema"."secret""#,
        r#""OtherSchema".secret"#,
        r#"other_schema."Secret""#,
        "`otherdb`.`secret`",
        "`OtherDb`.secret",
        "otherdb.`Secret`",
        "catalog.other_schema.secret",
        r#""catalog"."other_schema"."secret""#,
        "`catalog`.`otherdb`.`secret`",
    ] {
        for sql in [
            format!("SELECT * FROM {relation}"),
            format!("SELECT * FROM metrics m JOIN {relation} s ON m.id = s.id"),
            format!("SELECT * FROM {relation} s JOIN metrics m ON m.id = s.id"),
            format!("SELECT * FROM (SELECT * FROM {relation}) nested"),
            format!("SELECT * FROM metrics WHERE EXISTS (SELECT 1 FROM {relation})"),
            format!("SELECT (SELECT count(*) FROM {relation}) FROM metrics"),
            format!("WITH hidden AS (SELECT * FROM {relation}) SELECT * FROM hidden"),
            format!("SELECT * FROM metrics UNION ALL SELECT * FROM {relation}"),
        ] {
            assert!(
                validate_read_only_sql(&sql).is_err(),
                "qualified relation accepted: {sql}"
            );
        }
    }
}

#[test]
fn scheduled_sql_preserves_unqualified_relations_aliases_and_column_qualification() {
    use datazen_feature_core::dashboard::schedule_grant::validate_read_only_sql;
    for sql in [
        "SELECT * FROM metrics",
        "SELECT m.id FROM metrics AS m",
        r#"SELECT * FROM "Metrics""#,
        "SELECT * FROM `Metrics`",
        // The dot is inside one quoted identifier, not a namespace separator.
        r#"SELECT * FROM "literal.table""#,
        "SELECT * FROM `literal.table`",
        "SELECT m.id FROM metrics m JOIN samples s ON m.id = s.id",
        "SELECT * FROM (SELECT * FROM metrics) nested",
        "SELECT * FROM metrics WHERE EXISTS (SELECT 1 FROM samples)",
        "SELECT (SELECT count(*) FROM samples) FROM metrics",
        "WITH visible AS (SELECT * FROM metrics) SELECT * FROM visible",
        "SELECT * FROM metrics UNION ALL SELECT * FROM samples",
    ] {
        assert!(
            validate_read_only_sql(sql).is_ok(),
            "unqualified relation rejected: {sql}"
        );
    }
}
