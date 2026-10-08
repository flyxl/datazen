//! The apply Job must be addressable while it is still running.
//!
//! A cross-database migration can take minutes. When `apply_data_transfer_job`
//! awaited the write to a terminal state, the only thing the caller ever received
//! was a view of a Job that had already finished — so there was no handle to poll,
//! or to cancel with, and a stuck migration could only be waited out.
//!
//! Splitting a run into "admit" and "drive" is what makes an id addressable, and
//! the two halves have to be pinned separately, because neither can stand in for
//! the other:
//!
//! * [`an_apply_job_publishes_its_id_before_anything_is_written`] and its cancel
//!   sibling drive [`admit_apply`] directly, so they hold the returned view still
//!   while nothing has run. They prove the id is complete, readable and
//!   cancellable at the moment it is handed over.
//! * [`an_apply_command_returns_while_the_write_it_owes_is_still_blocked`] goes
//!   through [`apply_detached`] — the body the IPC command actually calls — and
//!   proves the *timing*: that the command answers while its write is still
//!   blocked. Only that test can tell a detached command from an awaited one.
//!
//! The second point is not pedantry. Both versions of `apply_detached` return the
//! same `queued` view, because `admit_apply` computes that view before any write
//! and awaiting a future does not change a value already in hand. A test that only
//! asserts on the returned state, or that polls with nothing but an upper bound,
//! therefore still passes against an apply that blocks its caller on the whole
//! migration.

use std::time::Duration;

use datazen_platform_api::dto::execution::EffectOutcome;
use datazen_platform_api::dto::job::{JobProgress, JobState};

use super::super::cancel::{cancel_data_transfer_job, job_cancel_requested};
use super::super::queries::read_job;
use super::super::runtime::{hold_write_closed, APPLY_KIND};
use super::super::{
    admit_apply, apply_detached, prepare_data_transfer_job_impl, TransferApplyJobRequest,
};
use super::{apply_request_from, direct_job, mock_options, prepare_request, TestAppState};

/// Admission hands over a complete, readable, cancellable handle — nothing more.
///
/// What this test deliberately does *not* claim, because admission makes it
/// impossible to claim: that the caller learns the id before the write starts.
/// `admit_apply` splits the run by construction — it returns the continuation
/// instead of running it — so the view it hands back is `queued` whether or not
/// whoever calls it spawns the write. A command that awaited the write inline is
/// not a different version of *this* function, and this test would pass against
/// one. That question is what
/// `an_apply_command_returns_while_the_write_it_owes_is_still_blocked` exists for.
#[tokio::test]
async fn an_apply_job_publishes_its_id_before_anything_is_written() {
    let test = TestAppState::with_options(mock_options()).await;
    let (_source_config, source) = test.save_and_connect("p5ja-early-source").await;
    let (_target_config, target) = test.save_and_connect("p5ja-early-target").await;
    let prepared =
        prepare_data_transfer_job_impl(&test.state, prepare_request(direct_job(&source, &target)))
            .await
            .expect("a direct-pair review should issue a plan");

    let admitted = admit_apply(&test.state, apply_request_from(&prepared))
        .await
        .expect("the apply Job must be admitted");
    let view = admitted.view;

    assert!(
        !view.job_id.is_empty(),
        "the caller must get a handle it can use, not an empty id"
    );
    assert_eq!(
        view.state,
        JobState::Queued,
        "admission is the moment the id exists; if the state is already terminal \
         the caller received it too late to do anything with it"
    );
    assert!(
        !view.state.is_terminal(),
        "a Job the caller can still cancel has not finished yet"
    );
    assert_eq!(
        view.effect_outcome,
        EffectOutcome::NotStarted,
        "nothing has been written, so no effect can have happened"
    );
    assert_eq!(
        view.progress,
        JobProgress::default(),
        "no rows have moved yet, so every counter is still zero"
    );
    assert!(
        view.commit_boundaries.is_empty(),
        "a boundary is a record of a write that already committed: {:?}",
        view.commit_boundaries
    );
    assert!(
        !view.cancelled && !view.partial && view.error.is_none(),
        "an admitted Job reports no cancellation, no partial write and no error: {view:?}"
    );

    // The id the caller just received is already a handle: the read side resolves
    // it and reports the same `queued` state.
    let read = read_job(&test.state, &view.job_id)
        .await
        .expect("an admitted Job must be readable by the id its caller holds");
    assert_eq!(read.job_id.as_str(), view.job_id);
    assert_eq!(read.kind, APPLY_KIND);
    assert_eq!(
        read.state,
        JobState::Queued,
        "the read side must agree with the view the caller received"
    );

    let finished = admitted
        .drive
        .expect("an admitted apply still owes a write")
        .finish()
        .await
        .expect("the detached write must report its verdict");
    assert_eq!(
        finished.job_id, view.job_id,
        "the write must land on the Job the caller was already told about"
    );
    assert_eq!(finished.state, JobState::Succeeded, "{:?}", finished.error);
}

/// Cancelling in the window between "id returned" and "write started" has to work.
///
/// This is the defect itself: before the split, that window did not exist, so a
/// cancel could only ever be aimed at a Job that had already settled — and
/// `cancel_data_transfer_job` deliberately answers `true` for a terminal Job
/// precisely because a settled state has nothing left to interrupt. Asserting that
/// the cancel is *recorded* (`cancel_requested`) and that the Job then ends
/// `Cancelled` with `notStarted` is what distinguishes "reached a running Job"
/// from "reached a finished one".
#[tokio::test]
async fn an_apply_job_can_be_cancelled_through_the_id_it_already_published() {
    let test = TestAppState::with_options(mock_options()).await;
    let (_source_config, source) = test.save_and_connect("p5ja-cancel-source").await;
    let (_target_config, target) = test.save_and_connect("p5ja-cancel-target").await;
    let prepared =
        prepare_data_transfer_job_impl(&test.state, prepare_request(direct_job(&source, &target)))
            .await
            .expect("a direct-pair review should issue a plan");

    let admitted = admit_apply(&test.state, apply_request_from(&prepared))
        .await
        .expect("the apply Job must be admitted");
    let job_id = admitted.view.job_id.clone();

    assert!(
        cancel_data_transfer_job(&test.state, &job_id)
            .await
            .expect("cancelling a known Job must not fail"),
        "the host accepted this id at admission, so it owns the cancel"
    );
    assert!(
        job_cancel_requested(&test.state, &job_id)
            .await
            .expect("a cancel lookup must not fail"),
        "the cancel must be recorded on the Job the caller is holding, \
         otherwise the caller has no way to know it landed"
    );

    // Queued cancellation is confirmed as NotStarted immediately, so the
    // persisted read model is already terminal before a detached dispatcher runs.
    let read = read_job(&test.state, &job_id)
        .await
        .expect("the cancelled Job is still readable");
    assert!(
        read.cancel_requested,
        "a caller that only polls getJob would never see the cancel otherwise"
    );
    assert_eq!(read.state, JobState::Cancelled);
    assert_eq!(read.effect_outcome, Some(EffectOutcome::NotStarted));

    let finished = admitted
        .drive
        .expect("an admitted apply still owes a write")
        .finish()
        .await
        .expect("a cancelled write still reports a verdict");
    assert_eq!(
        finished.state,
        JobState::Cancelled,
        "a cancel recorded before dispatch must terminate the Job, \
         not be ignored because the Job had not started yet"
    );
    assert!(finished.cancelled, "{finished:?}");
    assert_eq!(
        finished.effect_outcome,
        EffectOutcome::NotStarted,
        "a Job cancelled before it wrote has no effect to report"
    );
}

/// The continuation the command left behind finishes on its own, and the Job it
/// lands on is readable by the id the caller was already handed.
///
/// What this test does **not** establish is that the command did not block on the
/// write. It cannot: `apply_detached` returns the view `admit_apply` computed
/// before dispatching, so an `apply_detached` that awaited the write inline would
/// hand back the same `queued` view and pass every assertion below. The only thing
/// this test can fail on is a version that *dropped* the continuation instead of
/// spawning it, which would leave the Job `queued` forever — which is what the
/// bounded poll detects. The blocking half of the contract belongs to
/// `an_apply_command_returns_while_the_write_it_owes_is_still_blocked`.
#[tokio::test]
async fn a_detached_apply_finishes_on_its_own_and_becomes_readable() {
    let test = TestAppState::with_options(mock_options()).await;
    let (_source_config, source) = test.save_and_connect("p5ja-detach-source").await;
    let (_target_config, target) = test.save_and_connect("p5ja-detach-target").await;
    let prepared =
        prepare_data_transfer_job_impl(&test.state, prepare_request(direct_job(&source, &target)))
            .await
            .expect("a direct-pair review should issue a plan");

    let view = apply_detached(&test.state, apply_request_from(&prepared))
        .await
        .expect("a detached apply returns at admission");
    assert!(
        !view.state.is_terminal(),
        "the id has to be usable before the write ends, not only after: {view:?}"
    );
    assert!(!view.job_id.is_empty(), "an empty id is not a handle");

    let terminal = poll_until_terminal(&test.state, &view.job_id).await;
    assert_eq!(
        terminal.state,
        JobState::Succeeded,
        "the spawned write must reach the same terminal verdict an awaited one \
         would have returned: {terminal:?}"
    );
    assert_ne!(
        terminal.progress,
        JobProgress::default(),
        "a Job that succeeded after moving rows cannot still report zero progress"
    );
}

/// The command must hand back the Job id while the write it owes is still blocked.
///
/// This is the test the split exists for, and the only one in the repository that
/// can tell a detached apply from an awaited one.
///
/// The reason it needs a gate rather than a state assertion is that the returned
/// view is *identical* either way. `admit_apply` computes it before dispatching,
/// so `Ok(admitted.view)` is what the command returns whether the write goes to a
/// detached task or is awaited inline — awaiting a future does not retroactively
/// change a value already in hand. Asserting `state == Queued` therefore proves
/// nothing about detaching, and `poll_until_terminal`'s bound is an upper bound
/// only: a command that blocked on the write would still settle in time and pass.
///
/// Holding the write closed separates them by *timing instead*: an awaited apply
/// cannot answer at all while its write is blocked, while a detached one answers
/// at admission. So the gate here is held on the release of the write itself, one
/// line above `JobRuntime::run` in `runtime::drive` — the only step in the host
/// that writes. It is armed before the command is called and keyed by this
/// test's own plan id, so no other Job in the process is parked by it.
#[tokio::test]
async fn an_apply_command_returns_while_the_write_it_owes_is_still_blocked() {
    // Generous next to the work in front of it (plan lookup, plan claim, endpoint
    // services, handler registry) and far below the 30s a real cross-database
    // migration can take. The claim being tested is that the command returns
    // without the write at all, not that it returns quickly.
    const RETURN_DEADLINE: Duration = Duration::from_secs(10);
    // The write is unblocked immediately after the check below, so this only has
    // to outlast one task hop. It exists to turn "the continuation was never
    // spawned" into a failure instead of an indefinite hang.
    const BLOCKED_DEADLINE: Duration = Duration::from_secs(10);

    let test = TestAppState::with_options(mock_options()).await;
    let (_source_config, source) = test.save_and_connect("p5ja-detach-gate-source").await;
    let (_target_config, target) = test.save_and_connect("p5ja-detach-gate-target").await;
    let prepared =
        prepare_data_transfer_job_impl(&test.state, prepare_request(direct_job(&source, &target)))
            .await
            .expect("a direct-pair review should issue a plan");
    assert!(
        !prepared.plan_id.is_empty(),
        "the gate is keyed by plan id, so the review must publish one"
    );

    let gate = hold_write_closed(&prepared.plan_id);

    // With the write shut, this can only complete if the command answered without
    // running it. Under an inline `await` it parks on the gate and this times out.
    let view = tokio::time::timeout(
        RETURN_DEADLINE,
        apply_detached(&test.state, apply_request_from(&prepared)),
    )
    .await
    .expect("apply_data_transfer_job must answer without waiting for the write")
    .expect("a detached apply returns at admission");
    assert!(
        !view.job_id.is_empty(),
        "the caller must get a handle it can use, not an empty id"
    );

    // "Returned before the write" has to mean the write is actually parked, not
    // merely that it had not been scheduled yet.
    tokio::time::timeout(BLOCKED_DEADLINE, gate.wait_until_write_is_blocked())
        .await
        .expect("the detached continuation must reach the write on its own task");

    let blocked = read_job(&test.state, &view.job_id)
        .await
        .expect("an admitted Job stays readable while its write is blocked");
    assert_eq!(
        blocked.job_id.as_str(),
        view.job_id,
        "the read side must answer for the very id the caller was handed"
    );
    assert_eq!(
        blocked.state,
        JobState::Queued,
        "the write is still shut, so the Job cannot have started"
    );
    assert_eq!(
        blocked.effect_outcome, None,
        "no effect outcome is derived until a run finishes, and the run cannot \
         have finished — the gate is still holding the write: {blocked:?}"
    );
    assert_eq!(
        blocked.progress,
        JobProgress::default(),
        "no rows have moved yet, so every counter is still zero"
    );
    assert!(
        blocked.artifact_ids.is_empty(),
        "a write that has not run cannot have produced an artifact: {:?}",
        blocked.artifact_ids
    );

    gate.release();

    let terminal = poll_until_terminal(&test.state, &view.job_id).await;
    assert_eq!(
        terminal.job_id.as_str(),
        view.job_id,
        "the released write must land on the Job the caller was already told about"
    );
    assert_eq!(
        terminal.state,
        JobState::Succeeded,
        "detaching must not change the verdict, only when the caller learns it: {terminal:?}"
    );
    assert_ne!(
        terminal.progress,
        JobProgress::default(),
        "a Job that succeeded after moving rows cannot still report zero progress"
    );
}

/// Poll the read side until the Job settles.
///
/// Bounded on purpose: a Job that never leaves `queued` — because the write was
/// dropped instead of spawned — has to surface as a failure rather than hang.
async fn poll_until_terminal(
    state: &crate::commands::AppState,
    job_id: &str,
) -> datazen_platform_api::dto::job::JobView {
    let deadline = Duration::from_secs(30);
    let poll = Duration::from_millis(20);
    let started = std::time::Instant::now();
    loop {
        let view = read_job(state, job_id)
            .await
            .unwrap_or_else(|error| panic!("an admitted Job must stay readable: {error:?}"));
        if view.state.is_terminal() {
            return view;
        }
        assert!(
            started.elapsed() < deadline,
            "the detached write never settled: still {:?} after {deadline:?}",
            view.state
        );
        tokio::time::sleep(poll).await;
    }
}

/// A Job id this host never issued is not a transfer failure, it is a lookup miss.
///
/// The read side shares its error vocabulary with every other command, so a
/// caller can tell "you asked about a job I don't have" from "the query broke".
#[tokio::test]
async fn reading_an_unknown_job_id_is_reported_as_not_found() {
    let test = TestAppState::with_options(mock_options()).await;
    let error = read_job(
        &test.state,
        "transfer-dataTransferApply-00000000-0000-0000-0000-000000000000",
    )
    .await
    .expect_err("an id no run ever issued must not resolve");
    assert!(
        matches!(error, crate::commands::error::CommandError::NotFound(_)),
        "a lookup miss has its own error variant: {error:?}"
    );
    assert!(
        format!("{error:?}").contains("was not found"),
        "the message must name the miss instead of leaking a port error: {error:?}"
    );
}

/// The apply command accepts exactly the argument bag the desktop transport sends.
///
/// The frontend transport spreads the request object into the Tauri argument bag
/// and renames the method to snake_case, so `planId` arrives as a *top-level*
/// argument named `planId`, not nested under `request`. Deserializing the literal
/// payload the browser would send keeps that mapping observable from the Rust
/// side — and `deny_unknown_fields` means a renamed field would be a hard reject
/// here rather than a silently dropped value in production.
#[test]
fn the_apply_command_accepts_the_argument_bag_the_transport_sends() {
    let payload = serde_json::json!({
        "planId": "plan-1",
        "planDigest": "sha256:abc",
        "selectionRevision": 7,
        "selection": { "sourceTables": null },
        "confirmedDestructive": false,
        // Exactly `localBackendScope()` in `src/commands/transferJobs.ts`: the
        // desktop window sends this object on both branches, and its shape is
        // the one the host's `deny_unknown_fields` has to accept.
        "backendScope": {
            "sourceBackendScope": "local-desktop-backend",
            "targetBackendScope": "local-desktop-backend",
            "profileBackendScopes": []
        },
        "idempotencyKey": null,
    });
    let request: TransferApplyJobRequest =
        serde_json::from_value(payload.clone()).unwrap_or_else(|error| {
            panic!("the transport's payload must deserialize: {error}; {payload}")
        });
    assert_eq!(request.plan_id, "plan-1");
    assert_eq!(request.plan_digest, "sha256:abc");
    assert_eq!(request.selection_revision, 7);
    assert!(!request.confirmed_destructive);
    assert_eq!(
        request.backend_scope.source_backend_scope, "local-desktop-backend",
        "a scope the client nests must survive as the host's own field name"
    );
    assert!(
        request.backend_scope.profile_backend_scopes.is_empty(),
        "an empty profile list is a real value, not a missing field: {:?}",
        request.backend_scope
    );
    assert_eq!(
        request.idempotency_key, None,
        "an omitted key must be accepted, not required"
    );

    let mut without_key = payload.clone();
    without_key
        .as_object_mut()
        .expect("an object")
        .remove("idempotencyKey");
    serde_json::from_value::<TransferApplyJobRequest>(without_key)
        .expect("the caller may leave the idempotency key out entirely");

    let mut wrong_case = payload;
    wrong_case
        .as_object_mut()
        .expect("an object")
        .insert("plan_id".to_string(), serde_json::json!("plan-1"));
    let error = serde_json::from_value::<TransferApplyJobRequest>(wrong_case)
        .expect_err("the host must reject the Rust spelling rather than ignore it");
    assert!(
        error.to_string().contains("unknown field"),
        "the refusal must name the offending field: {error}"
    );
}
