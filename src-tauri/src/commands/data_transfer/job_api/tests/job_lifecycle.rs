//! The lifecycle of one transfer Job as the client sees it: cancelling a Job
//! the runtime is driving, and the recovery verdict an apply Job publishes over
//! its own commit boundaries.

use datazen_platform_api::context::OwnerRef;
use datazen_platform_api::dto::job::JobDefinition;
use datazen_platform_api::id::{ClientInstanceId, IdempotencyKey, JobId};
use datazen_platform_api::JobRepository;

use super::super::cancel::{cancel_data_transfer_job, job_cancel_requested};
use super::super::runtime::{host, now_timestamp, request_context, PREPARE_KIND};
use super::*;

/// §2.3 / D4: a cancel must land on the Job repository. The P5 runtime reads
/// `view.cancel_requested` and nothing else, and `services::job_registry` never
/// heard of these Job ids, so a cancel that skipped the repository was lost.
#[tokio::test]
async fn a_cancel_request_lands_on_the_job_repository() {
    // A queued Job is the state a cancel actually has to interrupt.
    let queued = JobId::new(format!(
        "transfer-dataTransferPrepare-{}",
        uuid::Uuid::new_v4()
    ));
    let ctx = request_context();
    host()
        .repo
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
        cancel_data_transfer_job(queued.as_str())
            .await
            .expect("a queued Job must accept a cancel request"),
        "the cancel must be taken through the Job repository"
    );
    assert!(
        job_cancel_requested(queued.as_str())
            .await
            .expect("the recorded cancel must be readable"),
        "the cancel must be recorded on the Job, not just returned as true"
    );

    // A finished Job is already decided: nothing to record, nothing to refuse.
    let test = TestAppState::with_options(mock_options()).await;
    let (_source_config, source) = test.save_and_connect("p5dt-cancel-source").await;
    let (_target_config, target) = test.save_and_connect("p5dt-cancel-target").await;
    let prepared =
        prepare_data_transfer_job_impl(&test.state, prepare_request(direct_job(&source, &target)))
            .await
            .expect("a direct-pair review should issue a plan");
    assert!(
        cancel_data_transfer_job(&prepared.job_id)
            .await
            .expect("a finished Job must not fail the cancel"),
        "the id is still a P5 Job, so the command answers for it"
    );
    assert!(
        !job_cancel_requested(&prepared.job_id)
            .await
            .expect("the recorded cancel must be readable"),
        "a Job that already finished has no cancel in effect"
    );

    // An id this client never accepted belongs to the legacy registry path.
    assert!(
        !cancel_data_transfer_job("transfer-apply-not-a-job")
            .await
            .expect("an unknown Job id is not an error"),
        "an unknown Job id must be refused so the caller can fall back"
    );
    assert!(
        !job_cancel_requested("transfer-apply-not-a-job")
            .await
            .expect("an unknown Job id is not an error"),
        "an unknown Job id has no recorded cancel"
    );
}

/// §7 / D6: the apply view must publish the verdict the handler reached over
/// its own evidence. The runtime writes `recovery_policy = "resumeAfterVerify"`
/// into every checkpoint it saves, so a verdict that echoed the checkpoint
/// would claim every transfer was resumable.
#[tokio::test]
async fn an_apply_job_publishes_a_recovery_verdict_over_its_own_boundaries() {
    let test = TestAppState::with_options(mock_options()).await;
    let (_source_config, source) = test.save_and_connect("p5dt-verdict-source").await;
    let (_target_config, target) = test.save_and_connect("p5dt-verdict-target").await;
    let prepared =
        prepare_data_transfer_job_impl(&test.state, prepare_request(direct_job(&source, &target)))
            .await
            .expect("a direct-pair review should issue a plan");
    let applied = apply_data_transfer_job_impl(&test.state, apply_request_from(&prepared))
        .await
        .expect("the apply Job should run");

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
        verdict @ ("reject" | "requireManualReview") => {
            assert_eq!(
                applied.recovery_resume_through, None,
                "{verdict} must not name a resume position"
            );
            let reason = applied
                .recovery_reason
                .as_deref()
                .filter(|reason| !reason.is_empty())
                .unwrap_or_else(|| panic!("{verdict} must say why"));
            assert!(
                reason.contains("resume") || reason.contains("evidence"),
                "the reason must name the missing evidence: {reason}"
            );
        }
        other => panic!("unknown recovery verdict: {other}"),
    }
}
