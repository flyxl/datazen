//! JobHandler 的**计划守卫**（§2.1 受理、§10.1.1 固定版本与授权）。
//!
//! 这里锁住的是「什么情况下允许开工」的判定，刻意不碰任何 host 端口：
//! * kind 双向不串（prepare handler 不接 apply 计划，反之亦然）；
//! * 三个版本字段任一未知 major 一律拒绝（不升级、不降级）；
//! * apply 必须 `consumedPlanId == spec.planId` 且 `selectionRevision == spec.selection_revision`；
//! * prepare 的 payload **不带** `consumedPlanId`（planId 只能被 apply 消费一次）；
//! * 非法选项与同库自同步在开工前被拒；
//! * `verify_recovery` 的四条裁决路径。

#[macro_use]
mod support;

use std::collections::HashMap;
use std::sync::Arc;

use datazen_data_sync::job::body::{apply_payload, prepare_payload, APPLY_KIND, PREPARE_KIND};
use datazen_data_sync::job::{ApplySpec, DataSyncHandler, DataSyncHost, PrepareSpec};
use datazen_data_sync::model::{SyncOptions, TableMapping};
use datazen_platform_api::dto::job::Checkpoint;
use datazen_platform_api::id::{JobId, JobStateVersion, StageId};
use datazen_runtime::job::{
    project_frozen_plan, CancelToken, FrozenPlan, JobError, JobHandler, RecoveryVerdict, StageSpec,
};

use support::host::{source_endpoint, target_endpoint, FakeHost};
use support::rows::{PLAN_ID, TABLE};

const REVISION: u64 = 1;

fn host() -> Arc<dyn DataSyncHost> {
    Arc::new(FakeHost::new())
}

fn prepare_spec() -> PrepareSpec {
    PrepareSpec {
        source: source_endpoint(),
        target: target_endpoint(),
        mappings: vec![TableMapping::auto(TABLE)],
        options: SyncOptions::default(),
        filters: HashMap::new(),
        plan_id: Some(PLAN_ID.to_string()),
    }
}

fn apply_spec() -> ApplySpec {
    ApplySpec {
        plan_id: PLAN_ID.to_string(),
        selection_revision: REVISION,
        options: SyncOptions::default(),
    }
}

fn prepare_handler() -> DataSyncHandler {
    DataSyncHandler::for_prepare(prepare_spec(), host())
}

fn apply_handler() -> DataSyncHandler {
    DataSyncHandler::for_apply(apply_spec(), host())
}

fn frozen_prepare_plan() -> FrozenPlan {
    project_frozen_plan(PREPARE_KIND, &prepare_payload(&prepare_spec()))
        .expect("prepare payload projects")
}

fn frozen_apply_plan() -> FrozenPlan {
    project_frozen_plan(APPLY_KIND, &apply_payload(&apply_spec())).expect("apply payload projects")
}

fn checkpoint(evidence: &[&str], committed: usize) -> Checkpoint {
    Checkpoint {
        job_id: JobId::new("job-1"),
        state_version: JobStateVersion::new(1),
        stable_target_fingerprint: "fp-1".into(),
        committed: (0..committed)
            .map(|i| datazen_platform_api::dto::job::CommitBoundary {
                stage_id: StageId::new("apply"),
                stable_target_fingerprint: "fp-1".into(),
                committed_at: datazen_platform_api::id::Timestamp::new("t0"),
                operation_id: None,
                batch_id: Some(format!("{PLAN_ID}:users:update:{i}")),
                payload_digest: None,
                evidence: Vec::new(),
                verified_at: None,
            })
            .collect(),
        verification_evidence: evidence.iter().map(|e| (*e).to_string()).collect(),
        recovery_policy: "resumeAfterVerify".into(),
    }
}

#[test]
fn each_handler_serves_exactly_its_own_kind_and_version() {
    let prepare = prepare_handler();
    let apply = apply_handler();
    assert_eq!(prepare.kind(), PREPARE_KIND);
    assert_eq!(apply.kind(), APPLY_KIND);
    assert_eq!(prepare.handler_version(), 1);
    assert_eq!(apply.handler_version(), 1);

    let stages = prepare
        .validate_plan(&frozen_prepare_plan())
        .expect("prepare plan accepted");
    assert_eq!(stages.len(), 1);
    assert_eq!(stages[0].kind, "prepare");
    assert!(stages[0].depends_on.is_empty());

    let stages = apply
        .validate_plan(&frozen_apply_plan())
        .expect("apply plan accepted");
    assert_eq!(stages.len(), 1);
    assert_eq!(stages[0].kind, "apply");
    assert!(stages[0].depends_on.is_empty());
}

#[test]
fn prepare_payload_never_carries_a_consumed_plan_id() {
    let payload = prepare_payload(&prepare_spec());
    assert!(
        payload.get("consumedPlanId").is_none(),
        "prepare 受理体不得写 consumedPlanId：{payload}"
    );
    let plan = frozen_prepare_plan();
    assert!(plan.consumed_plan_id.is_none());
    assert!(!plan.is_apply);

    let payload = apply_payload(&apply_spec());
    assert_eq!(payload["consumedPlanId"], PLAN_ID);
    assert_eq!(payload["selectionRevision"], REVISION);
    let plan = frozen_apply_plan();
    assert!(plan.is_apply);
    assert_eq!(plan.consumed_plan_id.as_deref(), Some(PLAN_ID));
    assert_eq!(plan.selection_revision, Some(REVISION));
}

#[test]
fn handler_refuses_the_other_kinds_plan() {
    let prepare = prepare_handler();
    assert_eq!(
        prepare.validate_plan(&frozen_apply_plan()),
        Err(JobError::PlanProjectionInvalid(format!(
            "kind mismatch: plan={APPLY_KIND} handler={PREPARE_KIND}"
        )))
    );
    let apply = apply_handler();
    assert_eq!(
        apply.validate_plan(&frozen_prepare_plan()),
        Err(JobError::PlanProjectionInvalid(format!(
            "kind mismatch: plan={PREPARE_KIND} handler={APPLY_KIND}"
        )))
    );
}

#[test]
fn unknown_plan_major_is_rejected_in_any_of_the_three_version_fields() {
    for bump in 0..3 {
        let mut plan = frozen_apply_plan();
        match bump {
            0 => plan.plan_version = 2,
            1 => plan.handler_version = 2,
            _ => plan.checkpoint_version = 2,
        }
        assert_eq!(
            apply_handler().validate_plan(&plan),
            Err(JobError::VersionIncompatible {
                got: 2,
                supported: 1
            }),
            "version field #{bump} must be guarded"
        );
    }
}

#[test]
fn apply_handler_requires_the_matching_plan_id_and_selection_revision() {
    let mut other_plan = frozen_apply_plan();
    other_plan.consumed_plan_id = Some("plan-other".into());
    assert_eq!(
        apply_handler().validate_plan(&other_plan),
        Err(JobError::PlanProjectionInvalid(
            "consumedPlanId mismatch with apply body".into()
        ))
    );

    let mut stale_revision = frozen_apply_plan();
    stale_revision.selection_revision = Some(REVISION + 1);
    assert_eq!(
        apply_handler().validate_plan(&stale_revision),
        Err(JobError::PlanProjectionInvalid(
            "selectionRevision mismatch with apply body".into()
        ))
    );

    let mut missing_revision = frozen_apply_plan();
    missing_revision.selection_revision = None;
    assert_eq!(
        apply_handler().validate_plan(&missing_revision),
        Err(JobError::PlanProjectionInvalid(
            "selectionRevision mismatch with apply body".into()
        ))
    );

    let mut prepare_shaped = frozen_apply_plan();
    prepare_shaped.is_apply = false;
    assert_eq!(
        apply_handler().validate_plan(&prepare_shaped),
        Err(JobError::PlanProjectionInvalid(
            "apply handler got a prepare plan".into()
        ))
    );

    let mut apply_shaped = frozen_prepare_plan();
    apply_shaped.is_apply = true;
    assert_eq!(
        prepare_handler().validate_plan(&apply_shaped),
        Err(JobError::PlanProjectionInvalid(
            "prepare handler got an apply plan".into()
        ))
    );
}

#[test]
fn invalid_sync_options_are_rejected_before_any_stage_is_produced() {
    let mut spec = prepare_spec();
    spec.options.batch_size = 0;
    let handler = DataSyncHandler::for_prepare(spec, host());
    assert_eq!(
        handler.validate_plan(&frozen_prepare_plan()),
        Err(JobError::PlanProjectionInvalid(
            "invalid sync options".into()
        ))
    );

    let mut spec = apply_spec();
    spec.options.batch_size = 1_001;
    let handler = DataSyncHandler::for_apply(spec, host());
    assert_eq!(
        handler.validate_plan(&frozen_apply_plan()),
        Err(JobError::PlanProjectionInvalid(
            "invalid sync options".into()
        ))
    );
}

#[test]
fn self_sync_of_one_database_is_rejected_as_endpoint_overlap() {
    let mut spec = prepare_spec();
    spec.target = spec.source.clone();
    let handler = DataSyncHandler::for_prepare(spec, host());
    assert!(matches!(
        handler.validate_plan(&frozen_prepare_plan()),
        Err(JobError::EndpointOverlap(_))
    ));
}

#[tokio::test]
async fn handler_refuses_to_run_a_stage_it_does_not_own() {
    let foreign = StageSpec {
        stage_id: StageId::new("transfer"),
        kind: "transfer".into(),
        depends_on: Vec::new(),
    };
    let err = prepare_handler()
        .run_stage(&foreign, &CancelToken::new())
        .await
        .expect_err("prepare handler must not serve a transfer stage");
    assert!(matches!(err, JobError::PlanProjectionInvalid(ref m) if m.contains("transfer")));

    let err = apply_handler()
        .run_stage(
            &StageSpec {
                stage_id: StageId::new("prepare"),
                kind: "prepare".into(),
                depends_on: Vec::new(),
            },
            &CancelToken::new(),
        )
        .await
        .expect_err("apply handler must not serve a prepare stage");
    assert!(matches!(err, JobError::PlanProjectionInvalid(ref m) if m.contains("prepare")));
}

#[test]
fn recovery_verdicts_route_unknown_and_stale_evidence_to_manual_paths() {
    let prepare = prepare_handler();
    let apply = apply_handler();

    assert!(matches!(
        apply.verify_recovery(&checkpoint(&[], 2)),
        RecoveryVerdict::ResumeAfterVerify { resume_through: 2 }
    ));
    assert!(matches!(
        apply.verify_recovery(&checkpoint(&["batchVerified"], 0)),
        RecoveryVerdict::ResumeAfterVerify { resume_through: 0 }
    ));
    assert!(matches!(
        apply.verify_recovery(&checkpoint(&["outcomeUnknown:batch-1"], 1)),
        RecoveryVerdict::RequireManualReview { ref reason } if reason == "unknownCommitBoundary"
    ));
    assert!(matches!(
        apply.verify_recovery(&checkpoint(&["cleanupNotConfirmed:tgt-4"], 1)),
        RecoveryVerdict::RequireManualReview { ref reason } if reason == "cleanupNotConfirmed"
    ));
    for stale in ["planStale", "sourceChanged"] {
        assert!(
            matches!(
                apply.verify_recovery(&checkpoint(&[stale], 3)),
                RecoveryVerdict::Reject { .. }
            ),
            "{stale} must not auto-resume"
        );
    }
    // outcomeUnknown 优先于 planStale：未知效果范围是更强的阻塞事实。
    assert!(matches!(
        apply.verify_recovery(&checkpoint(&["planStale", "outcomeUnknown"], 3)),
        RecoveryVerdict::RequireManualReview { ref reason } if reason == "unknownCommitBoundary"
    ));
    // prepare 同样只读裁决，语义一致。
    assert!(matches!(
        prepare.verify_recovery(&checkpoint(&["outcomeUnknown"], 0)),
        RecoveryVerdict::RequireManualReview { ref reason } if reason == "unknownCommitBoundary"
    ));
}
