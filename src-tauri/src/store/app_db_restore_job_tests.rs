//! Desktop durable admission uses the exact RestorePlan::payload entry point.
use super::{jobs::SqliteJobRepository, AppDb};
use datazen_feature_core::restore_job::{RestorePlan, RESTORE_APPLY_KIND};
use datazen_platform_api::{
    dto::job::JobDefinition,
    id::*,
    ports::job::JobRepository,
    target::{ExecutionTarget, NamespaceTarget},
    OwnerRef, RequestContext,
};
use datazen_runtime::job::SharedClock;
use serde_json::json;
use std::sync::Arc;

fn context() -> RequestContext {
    RequestContext::new(
        OrganizationId::new("restore-org"),
        PrincipalId::new("restore-owner"),
        None,
        ClientInstanceId::new("restore-client"),
        RequestId::new("restore-request"),
        None,
    )
}
fn plan() -> RestorePlan {
    RestorePlan {
        plan_id: "restore-plan".into(),
        target: ExecutionTarget::new(
            ConnectionId::new("restore-profile"),
            NamespaceTarget::new(
                Some("disposable".into()),
                None,
                Some("public".into()),
                vec![],
            ),
        ),
        config_revision: Counter::new(7),
        artifact_id: ArtifactId::new("restore-artifact"),
        input_sha256: "a".repeat(64),
        input_bytes: Counter::new(42),
        single_transaction: true,
        overwrite: false,
    }
}
fn definition(id: &str) -> JobDefinition {
    JobDefinition {
        job_id: JobId::new(id),
        kind: RESTORE_APPLY_KIND.into(),
        owner: OwnerRef::ClientSession {
            client_instance_id: context().client_instance_id,
            purpose: "restoreApply".into(),
        },
        // Same producer called by commands/restore_job.rs; no handwritten test envelope.
        payload: plan().payload(),
        created_at: Timestamp::new("2026-10-18T00:00:00Z"),
    }
}
fn repository(path: &std::path::Path) -> SqliteJobRepository {
    SqliteJobRepository::new(
        AppDb::open(path).unwrap(),
        Arc::new(SharedClock::at("2026-10-18T00:00:00Z")),
        300,
    )
}
#[tokio::test]
async fn restore_durable_admission_accepts_actual_payload_and_preserves_single_consumption() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = context();
    let repo = repository(dir.path());
    let original = definition("restore-job");
    let accepted = repo
        .accept(&ctx, original.clone(), &IdempotencyKey::new("accept"))
        .await
        .unwrap();
    assert_eq!(accepted.definition.payload, plan().payload());
    drop(repo);
    let reopened = repository(dir.path());
    let record = reopened.get(&ctx, original.job_id.clone()).await.unwrap();
    assert_eq!(record.definition.payload, original.payload);
    assert_eq!(
        reopened
            .accept(&ctx, original, &IdempotencyKey::new("accept"))
            .await
            .unwrap()
            .view
            .job_id,
        JobId::new("restore-job")
    );
    assert!(reopened
        .accept(
            &ctx,
            definition("restore-replay"),
            &IdempotencyKey::new("replay")
        )
        .await
        .is_err());
}
#[tokio::test]
async fn restore_durable_admission_rejects_untyped_or_inconsistent_metadata_before_consumption() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repository(dir.path());
    let ctx = context();
    let original = definition("restore-job");
    for (pointer, value) in [
        ("/sql", json!("synthetic script")),
        ("/restorePlan/path", json!("/synthetic/private.sql")),
        ("/restorePlan/sql", json!("synthetic script")),
        ("/restorePlan/password", json!("synthetic-secret")),
        ("/restorePlan/target/password", json!("synthetic-secret")),
        (
            "/restorePlan/target/namespace/file",
            json!("/synthetic/private.sql"),
        ),
        (
            "/restorePlan/target/object",
            json!({"kind":"table", "name":"t", "signature":null, "sql":"synthetic script"}),
        ),
        ("/restorePlan/inputSha256", json!("not-a-digest")),
        ("/restorePlan/inputBytes", json!("-1")),
        ("/restorePlan/singleTransaction", json!("yes")),
        (
            "/restorePlan/target/namespace/path",
            json!([{"password":"synthetic-secret"}]),
        ),
        ("/restorePlan/artifactId", json!("")),
        ("/restorePlan/target/connectionId", json!("")),
        ("/consumedPlanId", json!("different-plan")),
        ("/selectionRevision", json!(8)),
    ] {
        let mut invalid = original.clone();
        let (parent, key) = pointer.rsplit_once('/').unwrap();
        invalid
            .payload
            .pointer_mut(parent)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert(key.into(), value);
        assert!(
            repo.accept(&ctx, invalid, &IdempotencyKey::new("accept"))
                .await
                .is_err(),
            "invalid projection accepted at {pointer}"
        );
    }
    let mut wrong_kind = original.clone();
    wrong_kind.kind = "schemaDiffApply".into();
    assert!(repo
        .accept(&ctx, wrong_kind, &IdempotencyKey::new("accept"))
        .await
        .is_err());
    // Rejected metadata cannot consume the plan or poison its idempotency key.
    assert!(repo
        .accept(&ctx, original, &IdempotencyKey::new("accept"))
        .await
        .is_ok());
}
