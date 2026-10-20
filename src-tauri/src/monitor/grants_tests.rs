use super::*;
use crate::platform::policy::DesktopPolicy;
use datazen_platform_api::{
    id::ConnectionId,
    ports::policy::{AuthorizationAction, AuthorizationSubject, PolicyService},
    target::{ExecutionTarget, NamespaceTarget},
};
use std::sync::Arc;
fn config() -> crate::db::ConnectionConfig {
    crate::db::ConnectionConfig {
        id: "pg".into(),
        name: "synthetic profile".into(),
        database_type: "postgresql".into(),
        host: None,
        port: None,
        database: Some("app".into()),
        schema: None,
        username: None,
        password: None,
        ssl_mode: Default::default(),
        connection_timeout: 5,
        max_pool_size: 1,
        ssh_tunnel: None,
        tunnel_kind: None,
        tunnel_id: None,
        http_proxy_tunnel: None,
        websocket_tunnel: None,
        color_tag: None,
        group: None,
        last_connected_at: None,
        server_version: None,
        options: None,
        read_only: false,
        pinned: false,
    }
}
async fn seeded() -> (tempfile::TempDir, Arc<crate::store::Store>, MonitorGrant) {
    let temp = tempfile::tempdir().unwrap();
    let store = Arc::new(
        crate::store::Store::init_with_path(temp.path())
            .await
            .unwrap(),
    );
    store.save_connection(config()).await.unwrap();
    let db = store.app_db();
    let yaml="id: wf\nname: Fixed\ndescription: ''\nconnection: pg\ndatabase: app\nsteps:\n  - type: query\n    id: q\n    sql: SELECT 1\n";
    db.upsert_workflow(&crate::store::WorkflowRecord {
        id: "wf".into(),
        name: "Fixed".into(),
        description: "".into(),
        visibility: crate::store::WorkflowVisibility::User,
        definition_yaml: yaml.into(),
        created_at: "".into(),
        updated_at: "".into(),
    })
    .unwrap();
    db.upsert_dashboard(&crate::store::DashboardRecord {
        id: "d".into(),
        name: "Ops".into(),
        created_at: "".into(),
        updated_at: "".into(),
        layout_cols: 12,
        layout_row_height: 80,
        enabled: true,
        refresh_paused: false,
    })
    .unwrap();
    db.upsert_widget(&crate::store::WidgetRecord {
        id: "w".into(),
        dashboard_id: "d".into(),
        title: "Report".into(),
        workflow_id: "wf".into(),
        view_mode: "table".into(),
        chart_config_json: None,
        layout_x: 0,
        layout_y: 0,
        layout_w: 4,
        layout_h: 3,
        refresh_mode: "interval".into(),
        refresh_sec: Some(30),
        alert_json: None,
        enabled: true,
        sort_order: 0,
        created_at: "".into(),
        updated_at: "".into(),
    })
    .unwrap();
    let grant = MonitorGrant {
        id: "grant".into(),
        dashboard_id: "d".into(),
        widget_id: "w".into(),
        workflow_id: "wf".into(),
        workflow_revision: revision(yaml),
        organization_id: crate::platform::LOCAL_ORGANIZATION_ID.into(),
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
    };
    db.save_monitor_grant(&grant).unwrap();
    (temp, store, grant)
}
#[tokio::test]
async fn persisted_monitor_grant_drives_real_desktop_policy_and_read_only_target_checks() {
    let (_temp, store, grant) = seeded().await;
    let policy = DesktopPolicy::new(store.clone());
    let context = grant.context();
    assert_eq!(
        store.app_db().monitor_grant(&grant.id).unwrap().unwrap(),
        grant
    );
    assert!(store
        .app_db()
        .list_monitor_grants()
        .unwrap()
        .contains(&grant));
    let delegation = policy.resolve_delegation(&context).await.unwrap();
    assert_eq!(delegation.delegation, grant.delegation());
    let subject = AuthorizationSubject::Delegated(delegation.delegation);
    let target = &grant.scopes[0].target;
    assert!(policy
        .authorize_target(
            &context,
            subject.clone(),
            AuthorizationAction::Query,
            target,
            "query",
            &serde_json::json!({"sql":"SELECT 1"})
        )
        .await
        .unwrap()
        .is_allowed());
    assert!(policy
        .authorize_target(
            &context,
            subject.clone(),
            AuthorizationAction::Query,
            target,
            "query",
            &serde_json::json!({"sql":"DELETE FROM synthetic"})
        )
        .await
        .is_err());
    // This is the production policy used before Runtime resource admission and
    // again before physical dispatch, including grants saved before this policy.
    for sql in [
        "SELECT nextval('seq')",
        "SELECT setval('seq', 1)",
        "SELECT currval('seq')",
        "SELECT lastval()",
        "SELECT GET_LOCK('name', 0)",
        "SELECT RELEASE_LOCK('name')",
        "SELECT RELEASE_ALL_LOCKS()",
        "SELECT IS_FREE_LOCK('name')",
        "SELECT IS_USED_LOCK('name')",
        "SELECT pg_advisory_unlock_all()",
    ] {
        assert!(
            policy
                .authorize_target(
                    &context,
                    subject.clone(),
                    AuthorizationAction::Query,
                    target,
                    "query",
                    &serde_json::json!({"sql":sql})
                )
                .await
                .is_err(),
            "unapproved function passed production policy"
        );
    }
    assert!(policy
        .authorize_target(
            &context,
            subject.clone(),
            AuthorizationAction::Query,
            target,
            "query",
            &serde_json::json!({"sql":"SELECT COALESCE(AVG(value), 0), COUNT(*) FROM metrics"})
        )
        .await
        .unwrap()
        .is_allowed());
    let wrong = ExecutionTarget::new(ConnectionId::new("pg"), NamespaceTarget::database("other"));
    assert!(policy
        .authorize_target(
            &context,
            subject.clone(),
            AuthorizationAction::Query,
            &wrong,
            "query",
            &serde_json::json!({"sql":"SELECT 1"})
        )
        .await
        .is_err());
    assert!(!policy
        .authorize(&context, subject.clone(), AuthorizationAction::Write)
        .await
        .unwrap()
        .is_allowed());
    // Profile changes are read afresh immediately before target dispatch.
    store
        .platform_update("pg", Some(1), |previous, metadata| {
            metadata.enabled = false;
            Ok(previous.unwrap())
        })
        .await
        .unwrap();
    assert!(policy
        .authorize_target(
            &context,
            subject,
            AuthorizationAction::Query,
            target,
            "query",
            &serde_json::json!({"sql":"SELECT 1"})
        )
        .await
        .is_err());
}
#[tokio::test]
async fn grant_missing_expiry_revocation_schedule_disable_and_definition_change_fail_closed() {
    let (_temp, store, original) = seeded().await;
    let policy = DesktopPolicy::new(store.clone());
    let context = original.context();
    for case in ["expired", "revoked", "wrong-principal", "wrong-version"] {
        let mut grant = original.clone();
        match case {
            "expired" => grant.expires_at = chrono::Utc::now() - chrono::Duration::seconds(1),
            "revoked" => {
                grant.revoked = true;
                grant.revocation_version += 1;
            }
            "wrong-principal" => grant.service_principal = "other".into(),
            _ => grant.revocation_version += 1,
        }
        store.app_db().save_monitor_grant(&grant).unwrap();
        assert!(policy.resolve_delegation(&context).await.is_err(), "{case}");
    }
    store.app_db().save_monitor_grant(&original).unwrap();
    let mut missing = context.clone();
    missing.delegation_id = Some(datazen_platform_api::id::DelegationId::new("missing"));
    assert!(policy.resolve_delegation(&missing).await.is_err());
    let mut dashboard = store.app_db().get_dashboard("d").unwrap();
    dashboard.enabled = false;
    store.app_db().upsert_dashboard(&dashboard).unwrap();
    assert!(policy.resolve_delegation(&context).await.is_err());
    dashboard.enabled = true;
    store.app_db().upsert_dashboard(&dashboard).unwrap();
    assert!(policy.resolve_delegation(&context).await.is_ok());
    let mut workflow = store.app_db().get_workflow("wf").unwrap();
    workflow.definition_yaml.push_str("# edited\n");
    store.app_db().upsert_workflow(&workflow).unwrap();
    assert!(policy.resolve_delegation(&context).await.is_err());
}
