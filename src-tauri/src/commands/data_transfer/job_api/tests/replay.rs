//! Retrying an apply must return the recorded Job, not a refusal.
//!
//! The apply path claims the plan before it writes, and a claim is one-shot: once
//! a plan is claimed it can never be claimed again. A retry after the caller lost
//! the Job's result — a reloaded window, a dropped response — therefore arrives
//! at an already-claimed plan. The only way back is the idempotency receipt, which
//! records what the first attempt produced. That check has to run *before* the
//! claim, otherwise the retry is refused for a reason the caller can do nothing
//! about: the one recovery path the design offers is unreachable.
//!
//! So the ordering is the contract under test, and both halves of it are checked
//! here: the same key replays, and a *different* key still hits the wall. Without
//! the second half, "the retry succeeded" could mean anything from a correct
//! receipt lookup to a plan that was quietly reused.

use datazen_platform_api::dto::job::JobState;

use super::super::runtime::APPLY_KIND;
use super::super::{admit_apply, prepare_data_transfer_job_impl};
use super::{
    apply_and_wait, apply_request_from, direct_job, mock_options, prepare_request, TestAppState,
};

/// Reusing a key returns the Job the first attempt produced.
///
/// The plan is `Consumed` by the time the second call runs, so any check placed
/// after the claim refuses it. This test therefore fails with
/// "plan was already consumed" if the receipt lookup is moved back behind the
/// claim, which is exactly the shape the code had before the ordering was fixed.
#[tokio::test]
async fn a_repeated_idempotency_key_replays_the_recorded_job() {
    let test = TestAppState::with_options(mock_options()).await;
    let (_source_config, source) = test.save_and_connect("p5ja-replay-source").await;
    let (_target_config, target) = test.save_and_connect("p5ja-replay-target").await;
    let prepared =
        prepare_data_transfer_job_impl(&test.state, prepare_request(direct_job(&source, &target)))
            .await
            .expect("a direct-pair review should issue a plan");
    let plan_id = prepared.plan_id.clone();
    let mut request = apply_request_from(&prepared);
    request.idempotency_key = Some("p5ja-replay-key".to_string());

    let first = apply_and_wait(&test.state, request.clone())
        .await
        .expect("the first attempt runs the Job");
    assert_eq!(first.state, JobState::Succeeded, "{:?}", first.error);
    assert!(
        !first.replayed,
        "the attempt that owns the receipt is not a replay"
    );
    let first_artifacts = first.artifact_ids.clone();
    let first_progress = first.progress;

    test.state
        .connection_manager
        .release(&source)
        .await
        .unwrap();
    test.state
        .connection_manager
        .release(&target)
        .await
        .unwrap();
    let mut changed = request.clone();
    changed.selection_revision += 1;
    assert!(admit_apply(&test.state, changed)
        .await
        .err()
        .unwrap()
        .to_string()
        .contains("different payload"));

    let second = apply_and_wait(&test.state, request.clone())
        .await
        .unwrap_or_else(|error| {
            panic!(
                "a retry that lost the first result must recover from the \
                 receipt, not be refused for a consumed plan: {error:?}"
            )
        });
    assert_eq!(
        second.job_id, first.job_id,
        "the replay has to name the Job that already ran, not mint a second one"
    );
    assert_eq!(second.kind, APPLY_KIND);
    assert_eq!(
        second.state,
        JobState::Succeeded,
        "the recorded verdict is what the retry gets back: {:?}",
        second.error
    );
    assert!(
        second.replayed,
        "the caller has to be able to tell a replay from a fresh run, or it \
         will re-attach a progress watcher to a Job that already finished"
    );
    assert_eq!(
        second.plan_id, plan_id,
        "the replay describes the plan the first attempt consumed"
    );
    assert_eq!(
        second.artifact_ids, first_artifacts,
        "the recorded artifacts are the whole point of replaying: the first \
         response is what the caller lost"
    );
    assert_eq!(
        second.progress, first_progress,
        "a replay reports the recorded counters, not a fresh zeroed Job"
    );

    // Replaying a third time must be just as stable: the receipt is looked up,
    // never consumed by being read.
    let third = apply_and_wait(&test.state, request)
        .await
        .expect("a receipt that could only be read once would not be a receipt");
    assert_eq!(third.job_id, first.job_id);
    assert!(third.replayed);
}

/// The receipt is keyed on the key, not on the plan.
///
/// A different key on the same plan has no recorded result to return, so it has to
/// meet the real rule — one planId, one write — and be told to prepare again. If
/// this call *also* replayed, the recovery path would have quietly become "anyone
/// may re-run a consumed plan", which is a worse bug than the one it replaced.
#[tokio::test]
async fn a_different_key_on_the_same_plan_is_still_refused() {
    let test = TestAppState::with_options(mock_options()).await;
    let (_source_config, source) = test.save_and_connect("p5ja-nokey-source").await;
    let (_target_config, target) = test.save_and_connect("p5ja-nokey-target").await;
    let prepared =
        prepare_data_transfer_job_impl(&test.state, prepare_request(direct_job(&source, &target)))
            .await
            .expect("a direct-pair review should issue a plan");
    let mut request = apply_request_from(&prepared);
    request.idempotency_key = Some("p5ja-nokey-first".to_string());

    let first = apply_and_wait(&test.state, request.clone())
        .await
        .expect("the first attempt runs the Job");
    assert_eq!(first.state, JobState::Succeeded, "{:?}", first.error);

    request.idempotency_key = Some("p5ja-nokey-second".to_string());
    let error = apply_and_wait(&test.state, request)
        .await
        .expect_err("a second key is a second write, and the plan is already spent");
    let message = format!("{error:?}");
    assert!(
        message.contains("already consumed"),
        "the refusal has to name the one-shot plan rule so the caller knows to \
         prepare again: {message}"
    );
}

/// A replay is decided at admission, so it must not mint a Job or take a claim.
///
/// The receipt branch returns the recorded view with nothing left to run. If it
/// instead fell through to the normal path, it would consume a second claim on a
/// plan that has only one — so this asserts both halves: the replayed call
/// admits without error, and it leaves the plan in the state the first attempt
/// put it in rather than advancing it.
#[tokio::test]
async fn a_replay_is_decided_before_the_plan_is_claimed_again() {
    let test = TestAppState::with_options(mock_options()).await;
    let (_source_config, source) = test.save_and_connect("p5ja-admit-source").await;
    let (_target_config, target) = test.save_and_connect("p5ja-admit-target").await;
    let prepared =
        prepare_data_transfer_job_impl(&test.state, prepare_request(direct_job(&source, &target)))
            .await
            .expect("a direct-pair review should issue a plan");
    let mut request = apply_request_from(&prepared);
    request.idempotency_key = Some("p5ja-admit-key".to_string());

    let first = apply_and_wait(&test.state, request.clone())
        .await
        .expect("the first attempt runs the Job");
    assert_eq!(first.state, JobState::Succeeded, "{:?}", first.error);

    let admitted = admit_apply(&test.state, request)
        .await
        .expect("a replayed call is admitted, not refused");
    assert!(
        admitted.drive.is_none(),
        "a replayed call has nothing left to run; handing back a write to drive \
         would be a second attempt at a consumed plan"
    );
    assert_eq!(admitted.view.job_id, first.job_id);
    assert!(admitted.view.replayed);
    assert_eq!(admitted.view.state, JobState::Succeeded);
}
