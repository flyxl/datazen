//! T25 organization-memory API integration: real organization roles and
//! per-session organization isolation over the domain router.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request as HttpRequest, StatusCode};
use axum::Router;
use tower::ServiceExt;

use datazen_server::http::domains::org_memory::{org_memory_router, OrgMemoryState};
use datazen_server::http::middleware::auth::AuthenticatedSession;
use datazen_server::policy::{PolicyService, Role};
use datazen_server::storage::login_sessions::LoginSessionMode;
use datazen_server::storage::AdminStore;

fn store() -> Arc<AdminStore> {
    let store = AdminStore::open_in_memory().expect("store");
    store.migrate().expect("migrate");
    Arc::new(store)
}

fn policy(store: Arc<AdminStore>) -> Arc<PolicyService> {
    let policy = PolicyService::new(store, b"t25-test-server-secret".to_vec()).expect("policy");
    policy
        .create_membership("org-a", "admin-a", Role::OrgAdmin, "idp:admin-a")
        .expect("admin");
    policy
        .create_membership("org-a", "member-a", Role::OrgMember, "idp:member-a")
        .expect("member");
    policy
        .create_membership("org-b", "admin-b", Role::OrgAdmin, "idp:admin-b")
        .expect("other admin");
    Arc::new(policy)
}

fn app(store: Arc<AdminStore>, policy: Arc<PolicyService>) -> Router {
    org_memory_router(OrgMemoryState { store, policy })
}

fn session(org: &str, principal: &str) -> AuthenticatedSession {
    AuthenticatedSession {
        organization_id: org.into(),
        principal_id: principal.into(),
        session_id: format!("sess-{principal}"),
        permission_version: 1,
        token_epoch: 0,
        auth_mode: LoginSessionMode::Cookie,
    }
}

async fn request(
    app: &mut Router,
    method: &str,
    uri: &str,
    body: Option<&str>,
    session: Option<&AuthenticatedSession>,
) -> axum::http::Response<Body> {
    let mut builder = HttpRequest::builder().method(method).uri(uri);
    if body.is_some() {
        builder = builder.header(header::CONTENT_TYPE, "application/json");
    }
    let mut request = builder
        .body(Body::from(body.unwrap_or_default().to_owned()))
        .expect("request");
    if let Some(session) = session {
        request.extensions_mut().insert(session.clone());
    }
    app.oneshot(request).await.expect("oneshot")
}

async fn json_of(response: axum::http::Response<Body>) -> serde_json::Value {
    let bytes = axum::body::to_bytes(response.into_body(), 256 * 1024)
        .await
        .expect("body");
    serde_json::from_slice(&bytes).expect("json")
}

fn draft(subject: &str, content: &str) -> String {
    serde_json::json!({
        "kind": "glossary",
        "subject": subject,
        "content": content,
        "confidence": 0.9,
        "sensitivity": "normal"
    })
    .to_string()
}

#[tokio::test]
async fn members_read_but_only_org_admins_publish_and_revoke() {
    let store = store();
    let policy = policy(store.clone());
    let mut app = app(store, policy);
    let admin = session("org-a", "admin-a");
    let member = session("org-a", "member-a");

    let denied = request(
        &mut app,
        "POST",
        "/api/v1/org-memory",
        Some(&draft("Revenue", "Use paid_at")),
        Some(&member),
    )
    .await;
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);

    let published = request(
        &mut app,
        "POST",
        "/api/v1/org-memory",
        Some(&draft("Revenue", "Use paid_at")),
        Some(&admin),
    )
    .await;
    assert_eq!(published.status(), StatusCode::CREATED);
    let entry = json_of(published).await;
    let id = entry["id"].as_str().expect("id").to_string();
    assert_eq!(entry["version"], 1);
    assert_eq!(entry["approvedBy"], "admin-a");
    assert_eq!(entry["status"], "active");

    let list = request(&mut app, "GET", "/api/v1/org-memory", None, Some(&member)).await;
    assert_eq!(list.status(), StatusCode::OK);
    let body = json_of(list).await;
    assert_eq!(body["entries"].as_array().expect("entries").len(), 1);

    let denied_delete = request(
        &mut app,
        "DELETE",
        &format!("/api/v1/org-memory/{id}"),
        None,
        Some(&member),
    )
    .await;
    assert_eq!(denied_delete.status(), StatusCode::FORBIDDEN);

    let revoked = request(
        &mut app,
        "DELETE",
        &format!("/api/v1/org-memory/{id}"),
        None,
        Some(&admin),
    )
    .await;
    assert_eq!(revoked.status(), StatusCode::NO_CONTENT);
    let list = request(&mut app, "GET", "/api/v1/org-memory", None, Some(&member)).await;
    assert_eq!(
        json_of(list).await["entries"]
            .as_array()
            .expect("entries")
            .len(),
        0
    );
}

#[tokio::test]
async fn cross_organization_reads_and_revokes_are_invisible() {
    let store = store();
    let policy = policy(store.clone());
    let mut app = app(store, policy);
    let admin_a = session("org-a", "admin-a");
    let admin_b = session("org-b", "admin-b");
    let member_a = session("org-a", "member-a");

    let published = request(
        &mut app,
        "POST",
        "/api/v1/org-memory",
        Some(&draft("Org A only", "tenant-a")),
        Some(&admin_a),
    )
    .await;
    let id = json_of(published).await["id"]
        .as_str()
        .expect("id")
        .to_string();

    let list_b = request(&mut app, "GET", "/api/v1/org-memory", None, Some(&admin_b)).await;
    assert_eq!(
        json_of(list_b).await["entries"]
            .as_array()
            .expect("entries")
            .len(),
        0
    );
    let revoke_b = request(
        &mut app,
        "DELETE",
        &format!("/api/v1/org-memory/{id}"),
        None,
        Some(&admin_b),
    )
    .await;
    assert_eq!(revoke_b.status(), StatusCode::NOT_FOUND);

    let list_a = request(&mut app, "GET", "/api/v1/org-memory", None, Some(&member_a)).await;
    assert_eq!(
        json_of(list_a).await["entries"]
            .as_array()
            .expect("entries")
            .len(),
        1
    );
}

#[tokio::test]
async fn publish_is_versioned_and_audit_has_no_content() {
    let store = store();
    let policy = policy(store.clone());
    let mut app = app(store, policy);
    let admin = session("org-a", "admin-a");
    let member = session("org-a", "member-a");

    let first = request(
        &mut app,
        "POST",
        "/api/v1/org-memory",
        Some(&draft("Revenue", "sensitive-policy-v1")),
        Some(&admin),
    )
    .await;
    let first = json_of(first).await;
    let id = first["id"].as_str().expect("id").to_string();
    let second = request(
        &mut app,
        "POST",
        "/api/v1/org-memory",
        Some(
            &serde_json::json!({
                "memoryId": id,
                "kind": "glossary",
                "subject": "Revenue",
                "content": "sensitive-policy-v2",
                "confidence": 0.95,
                "sensitivity": "normal"
            })
            .to_string(),
        ),
        Some(&admin),
    )
    .await;
    assert_eq!(second.status(), StatusCode::CREATED);
    let second = json_of(second).await;
    assert_eq!(second["version"], 2);

    let list = request(&mut app, "GET", "/api/v1/org-memory", None, Some(&member)).await;
    let body = json_of(list).await;
    let entries = body["entries"].as_array().expect("entries");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["version"], 2);

    let audit = request(
        &mut app,
        "GET",
        "/api/v1/org-memory/audit",
        None,
        Some(&admin),
    )
    .await;
    assert_eq!(audit.status(), StatusCode::OK);
    let wire = json_of(audit).await.to_string();
    assert!(wire.contains("publish"));
    assert!(wire.contains("admin-a"));
    assert!(wire.contains(&id));
    assert!(wire.contains("\"version\":1"));
    assert!(wire.contains("\"version\":2"));
    assert!(!wire.contains("sensitive-policy-v1"));
    assert!(!wire.contains("sensitive-policy-v2"));

    let audit_member = request(
        &mut app,
        "GET",
        "/api/v1/org-memory/audit",
        None,
        Some(&member),
    )
    .await;
    assert_eq!(audit_member.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn injection_audit_is_suppressed_immediately_after_revoke() {
    let store = store();
    let policy = policy(store.clone());
    let mut app = app(store, policy);
    let admin = session("org-a", "admin-a");
    let member = session("org-a", "member-a");

    let published = request(
        &mut app,
        "POST",
        "/api/v1/org-memory",
        Some(&draft("Revenue", "Use paid_at")),
        Some(&admin),
    )
    .await;
    let entry = json_of(published).await;
    let id = entry["id"].as_str().expect("id").to_string();
    let version = entry["version"].as_u64().expect("version");

    let injected = request(
        &mut app,
        "POST",
        &format!("/api/v1/org-memory/{id}/injections"),
        Some(&serde_json::json!({"version": version, "decision": "injected"}).to_string()),
        Some(&member),
    )
    .await;
    assert_eq!(injected.status(), StatusCode::NO_CONTENT);

    let revoke = request(
        &mut app,
        "DELETE",
        &format!("/api/v1/org-memory/{id}"),
        None,
        Some(&admin),
    )
    .await;
    assert_eq!(revoke.status(), StatusCode::NO_CONTENT);

    let late = request(
        &mut app,
        "POST",
        &format!("/api/v1/org-memory/{id}/injections"),
        Some(&serde_json::json!({"version": version, "decision": "injected"}).to_string()),
        Some(&member),
    )
    .await;
    assert_eq!(late.status(), StatusCode::NO_CONTENT);

    let audit = request(
        &mut app,
        "GET",
        "/api/v1/org-memory/audit",
        None,
        Some(&admin),
    )
    .await;
    let audit = json_of(audit).await;
    let rows = audit["entries"].as_array().expect("entries");
    assert!(rows
        .iter()
        .any(|row| row["action"] == "inject" && row["decision"] == "suppressed"));
}

#[tokio::test]
async fn unauthenticated_requests_are_rejected() {
    let store = store();
    let policy = policy(store.clone());
    let mut app = app(store, policy);
    let response = request(&mut app, "GET", "/api/v1/org-memory", None, None).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
