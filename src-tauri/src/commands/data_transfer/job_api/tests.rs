//! Unit tests for the Job-shaped migration entry points.
//!
//! These cover the three contracts that must hold *before* any Job runs, so they
//! stay pure and synchronous: §8 same-backend admission, §2.1 prepare/apply
//! payload separation (checked against the runtime plan projector, not against a
//! copy of its rules), and the one-shot plan lifecycle in the plan store.

use std::collections::HashMap;

use datazen_runtime::job::{project_frozen_plan, EndpointRole};

use super::admission::{admit_apply_plan, ApplyPlanRequest, PlanAdmission, PlanAvailability};
use super::runtime::{endpoint_refs, APPLY_KIND, PREPARE_KIND};
use super::scope::{
    enforce_same_backend_scope, is_local_session_reference, TransferBackendScope,
    LOCAL_BACKEND_SCOPE,
};
use super::{
    apply_payload, availability_of, file_artifact_id, format_expiry, fresh_key, plan_digest,
    prepare_payload,
};
use crate::commands::data_transfer::plans::{self, PlanState};
use crate::data_transfer::model::{Endpoint, TransferOptions, TransferRunSelection};
use crate::data_transfer::{TableMapping, TransferJob, TransferMode, TransferPreview, WriteMode};

fn job(source_id: &str, target_id: &str) -> TransferJob {
    TransferJob {
        source: Endpoint {
            db_session_id: source_id.to_string(),
            database: "app".to_string(),
            schema: None,
        },
        target: Some(Endpoint {
            db_session_id: target_id.to_string(),
            database: "app".to_string(),
            schema: None,
        }),
        sql_file_target: None,
        mode: TransferMode::Data,
        write_mode: WriteMode::Insert,
        tables: vec![TableMapping::auto("users")],
        options: TransferOptions::default(),
    }
}

fn selection() -> TransferRunSelection {
    TransferRunSelection {
        source_tables: Some(vec!["users".to_string()]),
    }
}

fn preview() -> TransferPreview {
    TransferPreview {
        plan_id: String::new(),
        pairing_path: "direct".to_string(),
        mode: TransferMode::Data,
        write_mode: WriteMode::Insert,
        ddl: vec![],
        write_plans: vec![],
        warnings: vec![],
        can_execute: true,
        block_reason: None,
    }
}

fn admission(availability: PlanAvailability) -> PlanAdmission {
    PlanAdmission {
        plan_id: "plan-1".to_string(),
        plan_digest: "sha256:digest".to_string(),
        selection_revision: 7,
        expires_at_millis: 2_000,
        now_millis: 1_000,
        availability,
        can_execute: true,
        destructive: false,
    }
}

fn apply_request() -> ApplyPlanRequest {
    ApplyPlanRequest {
        plan_id: "plan-1".to_string(),
        plan_digest: "sha256:digest".to_string(),
        selection_revision: 7,
        confirmed_destructive: false,
    }
}

// ---------------------------------------------------------------- §2.1 payloads

#[test]
fn prepare_payload_declares_versions_and_consumes_nothing() {
    let payload = prepare_payload(3);
    assert_eq!(payload["kind"], PREPARE_KIND);
    assert_eq!(payload["planVersion"], 1);
    assert_eq!(payload["handlerVersion"], 1);
    assert_eq!(payload["checkpointVersion"], 1);
    assert_eq!(payload["selectionRevision"], 3);
    assert!(
        payload.get("consumedPlanId").is_none(),
        "a prepare Job must not claim to consume a plan"
    );
    assert!(
        project_frozen_plan(PREPARE_KIND, &payload).is_ok(),
        "the runtime projector must accept the prepare payload"
    );
    assert!(
        project_frozen_plan(APPLY_KIND, &payload).is_err(),
        "the prepare payload must not be usable as an apply payload"
    );
}

#[test]
fn apply_payload_projects_and_stays_a_reference() {
    let payload = apply_payload("plan-1", "sha256:digest", 7, &selection(), false);
    assert_eq!(payload["kind"], APPLY_KIND);
    assert_eq!(payload["consumedPlanId"], "plan-1");
    assert_eq!(payload["planDigest"], "sha256:digest");
    assert_eq!(payload["selectionRevision"], 7);
    assert_eq!(payload["sourceTables"][0], "users");
    assert_eq!(payload["confirmedDestructive"], false);
    assert!(
        project_frozen_plan(APPLY_KIND, &payload).is_ok(),
        "the runtime projector must accept the apply payload"
    );
    assert!(
        project_frozen_plan(PREPARE_KIND, &payload).is_err(),
        "an apply payload must not be usable as a prepare payload"
    );
}

#[test]
fn apply_payload_never_carries_endpoints_or_credentials() {
    let payload = apply_payload("plan-1", "sha256:digest", 7, &selection(), true);
    let text = payload.to_string();
    for forbidden in [
        "dbSessionId",
        "db_session_id",
        "session-secret",
        "app",
        "password",
        "connectionId",
        "host",
    ] {
        assert!(
            !text.contains(forbidden),
            "apply payload leaked `{forbidden}`: {text}"
        );
    }
    assert!(text.contains("confirmedDestructive"));
}

// ------------------------------------------------------------ §8 backend scope

#[test]
fn same_backend_scope_requires_the_local_declaration() {
    let err = enforce_same_backend_scope(&job("src-session", "tgt-session"), None)
        .expect_err("a missing declaration must be refused");
    assert!(
        err.to_string().contains("backendScope is required"),
        "unexpected reason: {err}"
    );
    assert!(enforce_same_backend_scope(
        &job("src-session", "tgt-session"),
        Some(&TransferBackendScope::local())
    )
    .is_ok());
}

#[test]
fn same_backend_scope_rejects_a_foreign_backend() {
    let foreign = TransferBackendScope {
        source_backend_scope: "queued-server".to_string(),
        target_backend_scope: LOCAL_BACKEND_SCOPE.to_string(),
        profile_backend_scopes: vec![],
    };
    let err = enforce_same_backend_scope(&job("src-session", "tgt-session"), Some(&foreign))
        .expect_err("a cross-backend migration must be refused");
    assert!(
        err.to_string().contains("queued-server"),
        "the refusal must name the offending scope: {err}"
    );

    let remote_profile = TransferBackendScope {
        source_backend_scope: LOCAL_BACKEND_SCOPE.to_string(),
        target_backend_scope: LOCAL_BACKEND_SCOPE.to_string(),
        profile_backend_scopes: vec![LOCAL_BACKEND_SCOPE.to_string(), "cloud-sync".to_string()],
    };
    let err = enforce_same_backend_scope(&job("src-session", "tgt-session"), Some(&remote_profile))
        .expect_err("a foreign profile scope must be refused");
    assert!(
        err.to_string().contains("cloud-sync"),
        "the refusal must name the offending profile: {err}"
    );
}

#[test]
fn same_backend_scope_rejects_non_local_endpoint_references() {
    for bad in [
        "backend/queue/job-1",
        "admin@remote-host",
        "https://example.invalid/db",
        "session with spaces",
        "",
    ] {
        let err = enforce_same_backend_scope(
            &job(bad, "tgt-session"),
            Some(&TransferBackendScope::local()),
        )
        .expect_err("`{bad}` must not pass as a local session token");
        assert!(
            err.to_string().contains("local backend"),
            "unexpected reason for `{bad}`: {err}"
        );
    }
    let oversized = "s".repeat(201);
    assert!(!is_local_session_reference(&oversized));
    assert!(is_local_session_reference("1f2e3d-session-token"));
}

// ------------------------------------------------------------------- admission

#[test]
fn admission_accepts_one_matching_review() {
    assert!(admit_apply_plan(&admission(PlanAvailability::Available), &apply_request()).is_ok());
}

#[test]
fn admission_reports_one_shot_consumption_before_expiry() {
    let mut stale = admission(PlanAvailability::Consumed);
    stale.expires_at_millis = 0;
    let err = admit_apply_plan(&stale, &apply_request())
        .expect_err("a consumed plan must not yield a second apply Job");
    assert!(
        err.to_string().contains("already consumed"),
        "unexpected reason: {err}"
    );

    let claimed = admission(PlanAvailability::Claimed);
    let err = admit_apply_plan(&claimed, &apply_request())
        .expect_err("a claimed plan must not yield a second apply Job");
    assert!(
        err.to_string().contains("one planId yields one Job"),
        "unexpected reason: {err}"
    );
}

#[test]
fn admission_refuses_expiry_digest_revision_and_drift() {
    let mut expired = admission(PlanAvailability::Available);
    expired.expires_at_millis = expired.now_millis;
    let err =
        admit_apply_plan(&expired, &apply_request()).expect_err("an expired plan must be refused");
    assert!(err.to_string().contains("expired"), "unexpected: {err}");

    let mut wrong_digest = apply_request();
    wrong_digest.plan_digest = "sha256:other".to_string();
    let err = admit_apply_plan(&admission(PlanAvailability::Available), &wrong_digest)
        .expect_err("a changed plan digest must be refused");
    assert!(
        err.to_string().contains("digest changed"),
        "unexpected: {err}"
    );

    let mut wrong_revision = apply_request();
    wrong_revision.selection_revision = 6;
    let err = admit_apply_plan(&admission(PlanAvailability::Available), &wrong_revision)
        .expect_err("a stale selection revision must be refused");
    assert!(
        err.to_string().contains("selection revision changed"),
        "unexpected: {err}"
    );

    let mut foreign_plan = apply_request();
    foreign_plan.plan_id = "plan-2".to_string();
    let err = admit_apply_plan(&admission(PlanAvailability::Available), &foreign_plan)
        .expect_err("a plan from another review must be refused");
    assert!(
        err.to_string().contains("not issued for this review"),
        "unexpected: {err}"
    );

    let mut blocked = admission(PlanAvailability::Available);
    blocked.can_execute = false;
    assert!(admit_apply_plan(&blocked, &apply_request()).is_err());
}

#[test]
fn destructive_writes_require_explicit_confirmation() {
    let mut destructive = admission(PlanAvailability::Available);
    destructive.destructive = true;
    let err = admit_apply_plan(&destructive, &apply_request())
        .expect_err("a destructive write must not run unconfirmed");
    assert!(
        err.to_string().contains("confirmedDestructive"),
        "unexpected: {err}"
    );
    let mut confirmed = apply_request();
    confirmed.confirmed_destructive = true;
    assert!(admit_apply_plan(&destructive, &confirmed).is_ok());
}

// ------------------------------------------------------------- plan lifecycle

#[test]
fn plan_digest_is_stable_and_covers_the_revision() {
    let driver = crate::testing::mock_driver::MockDriver::new("fixture", Default::default());
    let mut first = plans::issue_plan(
        job("src-session", "tgt-session"),
        &preview(),
        driver.as_ref(),
        driver.as_ref(),
        &HashMap::new(),
        &HashMap::new(),
        false,
    )
    .expect("plan issue");
    let digest = plan_digest(&plans::peek_plan_any(&first).expect("plan peek")).expect("digest");
    assert!(digest.starts_with("sha256:"), "unexpected digest: {digest}");
    assert_eq!(
        digest,
        plan_digest(&plans::peek_plan_any(&first).expect("plan peek")).expect("digest"),
        "the digest of one stored plan must be stable"
    );
    first = plans::issue_plan(
        job("src-session", "tgt-session"),
        &preview(),
        driver.as_ref(),
        driver.as_ref(),
        &HashMap::new(),
        &HashMap::new(),
        false,
    )
    .expect("plan issue");
    let redigest = plan_digest(&plans::peek_plan_any(&first).expect("plan peek")).expect("digest");
    assert_ne!(
        digest, redigest,
        "a re-issued plan is a different review and must not share a digest"
    );
}

#[test]
fn a_plan_id_serves_at_most_one_apply_job() {
    let driver = crate::testing::mock_driver::MockDriver::new("fixture", Default::default());
    let plan_id = plans::issue_plan(
        job("src-session", "tgt-session"),
        &preview(),
        driver.as_ref(),
        driver.as_ref(),
        &HashMap::new(),
        &HashMap::new(),
        false,
    )
    .expect("plan issue");
    assert_eq!(
        availability_of(plans::peek_plan_any(&plan_id).expect("peek").state),
        PlanAvailability::Available
    );

    plans::claim_plan(&plan_id).expect("claim");
    assert!(
        plans::peek_plan(&plan_id).is_err(),
        "a claimed plan is no longer previewable"
    );
    assert_eq!(
        availability_of(plans::peek_plan_any(&plan_id).expect("peek").state),
        PlanAvailability::Claimed
    );

    plans::mark_plan_consumed(&plan_id).expect("consume");
    let consumed = plans::peek_plan_any(&plan_id).expect("peek");
    assert_eq!(availability_of(consumed.state), PlanAvailability::Consumed);
    assert!(plans::peek_plan(&plan_id).is_err());
    assert!(
        plans::peek_plan_any("no-such-plan").is_err(),
        "an unknown plan id must not resolve"
    );
    assert_eq!(
        availability_of(PlanState::Available),
        PlanAvailability::Available
    );
    assert_eq!(
        availability_of(PlanState::Executing),
        PlanAvailability::Claimed
    );
}

// ------------------------------------------------------------- Job wiring facts

#[test]
fn both_endpoints_share_one_service_key_and_sql_file_has_no_writer() {
    let refs = endpoint_refs(
        vec!["users".to_string()],
        vec!["users_copy".to_string()],
        false,
    );
    assert_eq!(refs.len(), 2);
    assert_eq!(refs[0].role, EndpointRole::SourceReader);
    assert_eq!(refs[1].role, EndpointRole::TargetWriter);
    assert_eq!(
        refs[0].service_key, refs[1].service_key,
        "one service key makes a self-overlap a hard refusal"
    );

    let file_only = endpoint_refs(vec!["users".to_string()], Vec::new(), true);
    assert_eq!(file_only.len(), 1, "a SQL file run has no target endpoint");
    assert_eq!(file_only[0].role, EndpointRole::SourceReader);
    assert_eq!(file_only[0].objects, vec!["users".to_string()]);
}

#[test]
fn job_kinds_are_the_documented_names() {
    assert_eq!(PREPARE_KIND, "dataTransferPrepare");
    assert_eq!(APPLY_KIND, "dataTransferApply");
    assert!(datazen_runtime::job::APPLY_KINDS.contains(&APPLY_KIND));
    assert!(!datazen_runtime::job::APPLY_KINDS.contains(&PREPARE_KIND));
}

#[test]
fn emitted_sql_artifacts_are_content_addressed() {
    use sha2::{Digest, Sha256};
    let path = std::env::temp_dir().join(format!("p5dt-artifact-{}.sql", uuid::Uuid::new_v4()));
    let body = b"insert into users_copy select * from users;\n";
    std::fs::write(&path, body).expect("write fixture");
    let expected = format!("transfer-sql-{:x}", Sha256::digest(body));
    assert_eq!(file_artifact_id(&path).as_deref(), Some(expected.as_str()));
    std::fs::write(&path, b"-- changed\n").expect("rewrite fixture");
    assert_ne!(
        file_artifact_id(&path).as_deref(),
        Some(expected.as_str()),
        "a changed SQL file must not keep its artifact id"
    );
    let _ = std::fs::remove_file(&path);
    assert!(
        file_artifact_id(&path).is_none(),
        "a missing artifact has no id"
    );
}

#[test]
fn expiry_is_published_as_an_instant_and_keys_are_per_call() {
    let rendered = format_expiry(0);
    assert!(rendered.starts_with("1970-01-01T00:00:00"), "{rendered}");
    assert_eq!(
        format_expiry(i64::MAX),
        "1970-01-01T00:00:00+00:00",
        "an unrepresentable expiry must render a safe constant, never panic"
    );
    let first = fresh_key("plan-1");
    let second = fresh_key("plan-1");
    assert!(first.starts_with("plan-1-"), "{first}");
    assert_ne!(first, second, "a deliberate retry must be a new Job");
}
