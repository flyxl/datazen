//! `ServerProfiles` 行为钉子：与桌面 `DesktopProfiles` 同一用例语义的
//! 服务端腿（同一用例、两种绑定的第一组对齐断言）。
//!
//! 钉的语义：CM-05 越组织不可见、CAS 冲突取值、create 幂等重放、
//! disable 的版本递增、credential 版本独立递增、敏感公开选项拒绝。

use std::sync::Arc;

use datazen::connection::domain::{ProfileReader, ProfileRepository, ProfileScope};

// `get` / `list` 同时存在于两个 trait；统一走 `ProfileRepository::` 的全限定调用，
// `ProfileReader` 腿单独以全限定形式钉一次。
use datazen::connection::dto::ProfileDraft;
use datazen::shared::context::RequestContext;
use datazen::shared::error::PortError;
use datazen::shared::id::{
    ClientInstanceId, ConnectionId, IdempotencyKey, OrganizationId, PrincipalId, RequestId,
};

use super::profiles::ServerProfiles;
use super::AdminStore;

fn migrated_store() -> Arc<AdminStore> {
    let store = AdminStore::open_in_memory().expect("in-memory store");
    store.migrate().expect("migrations apply");
    Arc::new(store)
}

fn ctx(organization: &str, principal: &str) -> RequestContext {
    RequestContext {
        organization_id: OrganizationId::new(organization),
        principal_id: PrincipalId::new(principal),
        authentication_session_id: None,
        client_instance_id: ClientInstanceId::new("test-client"),
        request_id: RequestId::new("req-test"),
        delegation_id: None,
    }
}

fn draft(name: &str) -> ProfileDraft {
    ProfileDraft::new(name, "postgres")
}

#[tokio::test]
async fn create_persists_and_round_trips() {
    let store = migrated_store();
    let profiles = ServerProfiles::new(store);
    let context = ctx("org-a", "user-1");

    let record = profiles
        .create(&context, draft("primary"), &IdempotencyKey::new("idem-1"))
        .await
        .expect("create succeeds");

    assert_eq!(record.name, "primary");
    assert_eq!(record.driver_id, "postgres");
    assert_eq!(record.organization_id.as_str(), "org-a");
    assert_eq!(record.config_revision.counter().get(), 1);
    assert!(record.enabled);

    let loaded = ProfileRepository::get(
        &profiles,&context, record.connection_id.clone())
        .await
        .expect("get succeeds")
        .expect("record exists");
    assert_eq!(loaded.connection_id, record.connection_id);
    assert_eq!(loaded.name, "primary");
}

#[tokio::test]
async fn create_is_idempotent_per_key() {
    let store = migrated_store();
    let profiles = ServerProfiles::new(store);
    let context = ctx("org-a", "user-1");
    let idem = IdempotencyKey::new("idem-same");

    let first = profiles
        .create(&context, draft("one"), &idem)
        .await
        .expect("first create");
    let second = profiles
        .create(&context, draft("different payload"), &idem)
        .await
        .expect("replay succeeds (desktop semantics)");

    assert_eq!(
        first.connection_id, second.connection_id,
        "same key replays the original record"
    );
    assert_eq!(second.name, "one", "replay does not overwrite the payload");
}

#[tokio::test]
async fn cross_organization_access_is_invisible() {
    let store = migrated_store();
    let profiles = ServerProfiles::new(store);
    let org_a = ctx("org-a", "user-1");
    let org_b = ctx("org-b", "user-2");

    let record = profiles
        .create(&org_a, draft("a-only"), &IdempotencyKey::new("idem-a"))
        .await
        .expect("create in org-a");

    // CM-05：get 越组织 → None（不是 NotFound，不是错误）。
    assert!(
        ProfileRepository::get(
            &profiles,&org_b, record.connection_id.clone())
            .await
            .expect("get must not error")
            .is_none()
    );
    // CM-05：list 作用域越过请求组织 → 空。
    let scoped = ProfileRepository::list(
        &profiles,&org_b, ProfileScope::new(OrganizationId::new("org-a")))
        .await
        .expect("list must not error");
    assert!(scoped.is_empty());
    // 越组织的 CAS / disable 报 NotFound（不可区分「不存在」与「无权」）。
    match profiles
        .compare_and_set(
            &org_b,
            record.connection_id.clone(),
            datazen::shared::id::ConfigRevision::new(1),
            Default::default(),
        )
        .await
    {
        Err(PortError::NotFound(_)) => {}
        other => panic!("expected NotFound for cross-org CAS, got {other:?}"),
    }
    match profiles
        .disable(&org_b, record.connection_id.clone())
        .await
    {
        Err(PortError::NotFound(_)) => {}
        other => panic!("expected NotFound for cross-org disable, got {other:?}"),
    }
}

#[tokio::test]
async fn compare_and_set_enforces_revision_and_advances() {
    let store = migrated_store();
    let profiles = ServerProfiles::new(store);
    let context = ctx("org-a", "user-1");

    let record = profiles
        .create(&context, draft("v1"), &IdempotencyKey::new("idem-cas"))
        .await
        .expect("create");

    let stale = profiles
        .compare_and_set(
            &context,
            record.connection_id.clone(),
            datazen::shared::id::ConfigRevision::new(99),
            Default::default(),
        )
        .await
        .expect_err("stale revision must conflict");
    assert!(matches!(stale, PortError::CasConflict { .. }), "{stale:?}");

    let updated = profiles
        .compare_and_set(
            &context,
            record.connection_id.clone(),
            record.config_revision,
            datazen::connection::dto::ProfilePatch {
                name: Some("v2".into()),
                ..Default::default()
            },
        )
        .await
        .expect("cas succeeds");
    assert_eq!(updated.name, "v2");
    assert_eq!(updated.config_revision.counter().get(), 2);
    assert_eq!(
        updated.credential_revision.counter().get(),
        1,
        "no credentials in patch → credential revision untouched"
    );
}

#[tokio::test]
async fn credential_patch_advances_credential_revision_only_via_secret_ref() {
    let store = migrated_store();
    let profiles = ServerProfiles::new(store);
    let context = ctx("org-a", "user-1");

    let record = profiles
        .create(&context, draft("creds"), &IdempotencyKey::new("idem-cred"))
        .await
        .expect("create");

    let mut patch = datazen::connection::dto::ProfilePatch::default();
    patch.credentials = Some([("password".to_string(), "never-stored".to_string())].into());

    let updated = profiles
        .compare_and_set(&context, record.connection_id.clone(), record.config_revision, patch)
        .await
        .expect("cas with credentials");

    assert_eq!(
        updated.credential_revision.counter().get(),
        2,
        "credential patch bumps credential revision"
    );
    let secret = updated.secret_ref.expect("secret ref minted");
    assert!(
        secret.as_str().starts_with(&format!(
            "server-credential:{}:",
            record.connection_id.as_str()
        )),
        "secret ref is bound to the connection: {}",
        secret.as_str()
    );
}

#[tokio::test]
async fn disable_hides_from_list_and_bumps_config_revision() {
    let store = migrated_store();
    let profiles = ServerProfiles::new(store);
    let context = ctx("org-a", "user-1");

    let record = profiles
        .create(&context, draft("to-disable"), &IdempotencyKey::new("idem-off"))
        .await
        .expect("create");
    assert_eq!(
        ProfileRepository::list(
            &profiles,&context, ProfileScope::from_context(&context))
            .await
            .expect("list")
            .len(),
        1
    );

    let disabled = profiles
        .disable(&context, record.connection_id.clone())
        .await
        .expect("disable");
    assert!(!disabled.enabled);
    assert_eq!(disabled.config_revision.counter().get(), 2);

    assert!(
        ProfileRepository::list(
            &profiles,&context, ProfileScope::from_context(&context))
            .await
            .expect("list")
            .is_empty(),
        "disabled profiles leave the default list"
    );
    // get 仍然可见（disable ≠ 删除）。
    assert!(
        ProfileRepository::get(
            &profiles,&context, record.connection_id.clone())
            .await
            .expect("get")
            .is_some()
    );
}

#[tokio::test]
async fn advance_credential_revision_touches_only_credential_version() {
    let store = migrated_store();
    let profiles = ServerProfiles::new(store);
    let context = ctx("org-a", "user-1");

    let record = profiles
        .create(&context, draft("rotate"), &IdempotencyKey::new("idem-rot"))
        .await
        .expect("create");

    let revision = profiles
        .advance_credential_revision(&context, record.connection_id.clone())
        .await
        .expect("advance");
    assert_eq!(revision.counter().get(), 2);

    let after = ProfileRepository::get(
        &profiles,&context, record.connection_id.clone())
        .await
        .expect("get")
        .expect("still there");
    assert_eq!(after.credential_revision.counter().get(), 2);
    assert_eq!(
        after.config_revision.counter().get(),
        1,
        "credential rotation does not bump config revision"
    );
}

#[tokio::test]
async fn sensitive_public_options_are_rejected() {
    let store = migrated_store();
    let profiles = ServerProfiles::new(store);
    let context = ctx("org-a", "user-1");

    let mut poisoned = draft("bad");
    poisoned
        .public_options
        .insert("password".into(), serde_json::json!("nope"));
    let error = profiles
        .create(&context, poisoned, &IdempotencyKey::new("idem-bad"))
        .await
        .expect_err("sensitive option must be rejected");
    assert!(
        matches!(error, PortError::BackendUnavailable(ref message) if message.contains("sensitive")),
        "{error:?}"
    );
    assert!(
        ProfileRepository::list(
            &profiles,&context, ProfileScope::from_context(&context))
            .await
            .expect("list")
            .is_empty(),
        "rejected create leaves no row"
    );
}

#[tokio::test]
async fn unknown_profile_reads_are_none_not_errors() {
    let store = migrated_store();
    let profiles = ServerProfiles::new(store);
    let context = ctx("org-a", "user-1");

    assert!(
        ProfileRepository::get(
            &profiles,&context, ConnectionId::new("does-not-exist"))
            .await
            .expect("get on missing id is Ok")
            .is_none()
    );
    match profiles
        .advance_credential_revision(&context, ConnectionId::new("does-not-exist"))
        .await
    {
        Err(PortError::NotFound(_)) => {}
        other => panic!("expected NotFound, got {other:?}"),
    }
}
