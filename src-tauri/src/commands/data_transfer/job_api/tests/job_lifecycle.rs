//! The lifecycle of one transfer Job as the client sees it: cancelling a Job
//! the runtime is driving, and the recovery verdict an apply Job publishes over
//! its own commit boundaries.

use datazen_platform_api::context::OwnerRef;
use datazen_platform_api::dto::execution::EffectOutcome;
use datazen_platform_api::dto::job::{JobDefinition, JobProgress, JobState};
use datazen_platform_api::id::{ClientInstanceId, IdempotencyKey, JobId};

use super::super::cancel::{cancel_data_transfer_job, job_cancel_requested};
use super::super::runtime::{now_timestamp, request_context, PREPARE_KIND};
use super::*;

/// A cancel must land on the Job repository, not on a local registry: the P5
/// runtime reads `view.cancel_requested` and nothing else, so a cancel that
/// never touched the repository silently left the Job running. The IPC command
/// keeps that guarantee and answers `false` for ids the repository does not
/// know (see `cancel_command_answers_false_for_unknown_ids`).
#[tokio::test]
async fn a_cancel_request_lands_on_the_job_repository() {
    let test = TestAppState::with_options(mock_options()).await;
    // A queued Job is the state a cancel actually has to interrupt.
    let queued = JobId::new(format!(
        "transfer-dataTransferPrepare-{}",
        uuid::Uuid::new_v4()
    ));
    let ctx = request_context();
    test.state
        .desktop_job_host
        .accept(
            &ctx,
            JobDefinition {
                job_id: queued.clone(),
                kind: PREPARE_KIND.to_string(),
                owner: OwnerRef::ClientSession {
                    client_instance_id: ClientInstanceId::new("datazen-local-client"),
                    purpose: PREPARE_KIND.to_string(),
                },
                payload: serde_json::json!({
                    "planVersion": 1,
                    "handlerVersion": 1,
                    "checkpointVersion": 1,
                }),
                created_at: now_timestamp(),
            },
            &IdempotencyKey::new(format!("idem-{}", uuid::Uuid::new_v4())),
        )
        .await
        .expect("a queued Job should be accepted");

    assert!(
        cancel_data_transfer_job(&test.state, queued.as_str())
            .await
            .expect("a queued Job must accept a cancel request"),
        "the cancel must be taken through the Job repository"
    );
    assert!(
        job_cancel_requested(&test.state, queued.as_str())
            .await
            .expect("the recorded cancel must be readable"),
        "the cancel must be recorded on the Job, not just returned as true"
    );

    // A finished Job is already decided: nothing to record, nothing to refuse.
    let (_source_config, source) = test.save_and_connect("p5dt-cancel-source").await;
    let (_target_config, target) = test.save_and_connect("p5dt-cancel-target").await;
    let prepared =
        prepare_data_transfer_job_impl(&test.state, prepare_request(direct_job(&source, &target)))
            .await
            .expect("a direct-pair review should issue a plan");
    assert!(
        cancel_data_transfer_job(&test.state, &prepared.job_id)
            .await
            .expect("a finished Job must not fail the cancel"),
        "the id is still a P5 Job, so the command answers for it"
    );
    assert!(
        !job_cancel_requested(&test.state, &prepared.job_id)
            .await
            .expect("the recorded cancel must be readable"),
        "a Job that already finished has no cancel in effect"
    );

    // An id this backend never accepted is a Job-path refusal, not an error:
    // the repository answers `false` and has nothing to record, and the IPC
    // command on top of it also answers `false` ("nothing was cancelled").
    assert!(
        !cancel_data_transfer_job(&test.state, "transfer-apply-not-a-job")
            .await
            .expect("an unknown Job id is not an error"),
        "an unknown Job id must be refused, not fabricated into a cancel"
    );
    assert!(
        !job_cancel_requested(&test.state, "transfer-apply-not-a-job")
            .await
            .expect("an unknown Job id is not an error"),
        "an unknown Job id has no recorded cancel"
    );
}

/// The IPC cancel command must answer through the durable Job path and, for an
/// id this backend never accepted, honestly report that nothing was cancelled —
/// the never-`Ok(true)` shape that historically let a cancel look acknowledged
/// while the legacy registry had nothing registered under that id.
#[tokio::test]
async fn cancel_command_answers_false_for_unknown_ids() {
    let test = TestAppState::with_options(mock_options()).await;
    let unknown = "transfer-cancel-never-accepted";
    let answered = crate::commands::data_transfer::cancel_data_transfer_impl(&test.state, unknown)
        .await
        .expect("an unknown job id must not be a transport error");
    assert!(
        !answered,
        "false means nothing was cancelled so the caller must not treat it as success"
    );
}

/// The apply view must publish the verdict the handler reached over its own
/// evidence. The runtime writes `recovery_policy = "resumeAfterVerify"` into
/// every checkpoint it saves, so a verdict that merely echoed the checkpoint
/// would claim every transfer was resumable.
///
/// The run has to have actually moved rows before any of that means anything. An
/// earlier version of this test accepted all three verdicts and asserted nothing
/// about the outcome, which let a write that failed before reading a single row
/// still look like a healthy one — the test would have passed on a Job that
/// copied nothing. So the state comes first: a run that ended `Failed` is a
/// broken fixture, not a verdict to be explained.
#[tokio::test]
async fn an_apply_job_publishes_a_recovery_verdict_over_its_own_boundaries() {
    let test = TestAppState::with_options(mock_options()).await;
    let (_source_config, source) = test.save_and_connect("p5dt-verdict-source").await;
    let (_target_config, target) = test.save_and_connect("p5dt-verdict-target").await;
    let prepared =
        prepare_data_transfer_job_impl(&test.state, prepare_request(direct_job(&source, &target)))
            .await
            .expect("a direct-pair review should issue a plan");
    let applied = apply_and_wait(&test.state, apply_request_from(&prepared))
        .await
        .expect("the apply Job should run");

    assert_eq!(
        applied.state,
        JobState::Succeeded,
        "the write must finish before its verdict is worth reading: {:?}",
        applied.error
    );
    assert_eq!(
        applied.effect_outcome,
        EffectOutcome::Completed,
        "a completed copy is the only state that earns a resume verdict"
    );
    assert_ne!(
        applied.progress,
        JobProgress::default(),
        "a finished copy that counted no rows moved nothing: {:?}",
        applied.progress
    );

    match applied.recovery_verdict.as_str() {
        "resumeAfterVerify" => {
            assert!(
                applied.recovery_reason.is_none(),
                "a resume carries no refusal reason: {:?}",
                applied.recovery_reason
            );
            assert_eq!(
                applied.recovery_resume_through,
                Some(applied.commit_boundaries.len()),
                "a resume may pass through exactly the confirmed boundaries"
            );
            assert!(
                !applied.commit_boundaries.is_empty(),
                "nothing committed means there is nothing to resume through"
            );
        }
        verdict @ ("reject" | "requireManualReview") => panic!(
            "a Job that completed every boundary must not answer `{verdict}`: {:?}",
            applied.recovery_reason
        ),
        other => panic!("unknown recovery verdict: {other}"),
    }
}
