//! Independent audit for T391 (the useSchemaDiffJobLifecycle port).
//!
//! Tester-written: every assertion comes from re-reading
//! `packages/frontend/src/windows/schema-diff/useSchemaDiffJobLifecycle.ts`
//! and its React tests (`__tests__/useSchemaDiffEndpoints.test.tsx`,
//! `SchemaDiffWindow.test.tsx`): the tracked-job identity rules (same job
//! refreshes, a different job never replaces), the latest-apply capture,
//! the hydration's latest-first ordering + selection priority + ordered
//! reads + the 1000ms poll, and the cancel/verify outcome transitions.

use serde_json::json;

use datazen_ui::commands::schema_diff::wire::{
    JobRecoveryVerdictVm, SchemaDiffJobDetailsVm, parse_job_accepted, parse_job_views,
};
use datazen_ui::views::data_transfer::data_transfer_attached_jobs::JobState;
use datazen_ui::views::schema_diff::use_schema_diff_job_lifecycle::{
    LifecycleEffect, SchemaDiffCancelOutcome, SchemaDiffJobLifecycle, SCHEMA_DIFF_JOB_POLL_MS,
};

fn view_json(job_id: &str, kind: &str, state: &str, updated_at: i64) -> serde_json::Value {
    json!({
        "jobId": job_id,
        "kind": kind,
        "state": state,
        "progress": { "read": "0", "converted": "0", "attempted": "1", "committed": "1", "unknown": "0" },
        "updatedAt": updated_at,
        "error": null
    })
}

fn details_json(job_id: &str, kind: &str, state: &str, recovery: Option<serde_json::Value>) -> serde_json::Value {
    let mut payload = json!({
        "details": {
            "job": view_json(job_id, kind, state, 5),
            "stateVersion": 1,
            "recoveryTargets": [{ "connectionId": "c1", "objectIds": [] }]
        },
        "prepared": null,
        "planUnavailableAfterRestart": false,
        "deployResult": null
    });
    if let Some(recovery) = recovery {
        payload["details"]["recovery"] = recovery;
    }
    payload
}

fn details(job_id: &str, kind: &str, state: &str) -> SchemaDiffJobDetailsVm {
    datazen_ui::commands::schema_diff::normalize_job_details(details_json(job_id, kind, state, None))
        .unwrap()
}

/// A fresh local submission resets the cancel outcome and replaces the
/// tracked job with no details yet.
#[test]
fn accept_resets_and_tracks_locally() {
    let mut lifecycle = SchemaDiffJobLifecycle::default();
    lifecycle.on_cancel_error("boom".to_owned());
    assert_eq!(lifecycle.cancel_outcome(), SchemaDiffCancelOutcome::Unavailable);

    lifecycle.accept(parse_job_accepted(json!({
        "jobId": "j1", "kind": "schemaDiffPrepare", "state": "queued",
        "progress": { "read": 0, "converted": 0, "attempted": 0, "committed": 0, "unknown": 0 }
    })).unwrap());
    let job = lifecycle.current_job().expect("tracked");
    assert_eq!(job.job_id, "j1");
    assert!(job.details.is_none());
    assert!(job.submitted_locally);
    assert_eq!(lifecycle.cancel_outcome(), SchemaDiffCancelOutcome::Idle);
}

/// The tracked job refreshes only for the SAME job; a different job's
/// details never replace it. Apply details become latestApply.
#[test]
fn observe_identity_rules_and_latest_apply() {
    let mut lifecycle = SchemaDiffJobLifecycle::default();
    lifecycle.accept(parse_job_accepted(json!({
        "jobId": "prepare-1", "kind": "schemaDiffPrepare", "state": "running",
        "progress": { "read": 0, "converted": 0, "attempted": 0, "committed": 0, "unknown": 0 }
    })).unwrap());

    // Same job: details attach, submitted_locally survives.
    lifecycle.observe(details("prepare-1", "schemaDiffPrepare", "running"));
    let job = lifecycle.current_job().expect("tracked");
    assert!(job.details.is_some());
    assert!(job.submitted_locally);

    // A different job: ignored (no replace, no details swap) — and being a
    // prepare job it must not touch latestApply either.
    lifecycle.observe(details("other-9", "schemaDiffPrepare", "succeeded"));
    let job = lifecycle.current_job().expect("tracked");
    assert_eq!(job.job_id, "prepare-1");
    assert!(lifecycle.latest_apply().is_none(), "prepare details never set latestApply");

    // An apply job's details become latestApply — but the tracked job
    // STAYS prepare-1 (React keeps `current` when the ids differ).
    lifecycle.observe(details("apply-1", "schemaDiffApply", "succeeded"));
    let latest = lifecycle.latest_apply().expect("latest apply");
    assert_eq!(latest.details.job_id, "apply-1");
    assert_eq!(lifecycle.current_job().unwrap().job_id, "prepare-1");

    // With NO tracked job, details seed the tracked slot (React's
    // `current && … : {…}` else branch) without the local marker.
    let mut fresh = SchemaDiffJobLifecycle::default();
    fresh.observe(details("apply-2", "schemaDiffApply", "succeeded"));
    let job = fresh.current_job().expect("tracked");
    assert_eq!(job.job_id, "apply-2");
    assert!(!job.submitted_locally);
    assert!(fresh.latest_apply().is_some());
}

/// The bus subscription routes accepted and details updates.
#[test]
fn bus_update_routes() {
    use datazen_ui::commands::schema_diff::SchemaDiffJobUpdateVm;
    let mut lifecycle = SchemaDiffJobLifecycle::default();
    lifecycle.on_bus_update(SchemaDiffJobUpdateVm::Accepted(
        parse_job_accepted(json!({
            "jobId": "j2", "kind": "schemaDiffApply", "state": "queued",
            "progress": { "read": 0, "converted": 0, "attempted": 0, "committed": 0, "unknown": 0 }
        })).unwrap(),
    ));
    assert_eq!(lifecycle.current_job().unwrap().job_id, "j2");
    lifecycle.on_bus_update(SchemaDiffJobUpdateVm::Details(details("j2", "schemaDiffApply", "running")));
    assert!(lifecycle.current_job().unwrap().details.is_some());
}

/// The hydration: latest-first order, the active > pending > apply >
/// prepare selection, ordered reads, then the 1000ms poll while the job
/// stays active.
#[test]
fn hydrate_reads_selected_and_apply_then_polls() {
    let mut lifecycle = SchemaDiffJobLifecycle::default();
    assert_eq!(lifecycle.hydrate_begin(), vec![LifecycleEffect::ListJobs]);

    // Empty list: done.
    assert!(lifecycle.on_list_jobs(Vec::new()).is_empty());
    assert_eq!(lifecycle.hydrate_begin(), vec![LifecycleEffect::ListJobs]);

    // latestFirst: the apply job (older stamp) reads after the selected
    // active prepare job.
    let jobs = parse_job_views(json!([
        view_json("apply-old", "schemaDiffApply", "succeeded", 10),
        view_json("prepare-active", "schemaDiffPrepare", "running", 20),
    ]))
    .unwrap();
    let effects = lifecycle.on_list_jobs(jobs);
    assert_eq!(
        effects,
        vec![
            LifecycleEffect::GetJobDetails("prepare-active".to_owned()),
            LifecycleEffect::GetJobDetails("apply-old".to_owned()),
        ]
    );

    // The first read observes and advances; the queue keeps draining.
    let effects = lifecycle.on_details(1_000, details("prepare-active", "schemaDiffPrepare", "running"));
    assert!(effects.is_empty());
    let effects = lifecycle.on_details(1_050, details("apply-old", "schemaDiffApply", "succeeded"));
    // The queue drained with an active job: the 1000ms poll arms.
    assert_eq!(
        effects,
        vec![LifecycleEffect::Wait { until_ms: 1_050 + SCHEMA_DIFF_JOB_POLL_MS }]
    );
    // Before the deadline: nothing; at it: the poll read fires.
    assert!(lifecycle.tick(1_900).is_empty());
    assert_eq!(
        lifecycle.tick(2_050),
        vec![LifecycleEffect::GetJobDetails("prepare-active".to_owned())]
    );
    // Still running: re-arm. Terminal: the loop stops.
    let effects = lifecycle.on_poll_details(2_050, &details("prepare-active", "schemaDiffPrepare", "running"));
    assert_eq!(
        effects,
        vec![LifecycleEffect::Wait { until_ms: 2_050 + SCHEMA_DIFF_JOB_POLL_MS }]
    );
    let effects = lifecycle.on_poll_details(3_100, &details("prepare-active", "schemaDiffPrepare", "succeeded"));
    assert!(effects.is_empty());

    // The apply details landed as latestApply through the same reads.
    assert_eq!(lifecycle.latest_apply().unwrap().details.job_id, "apply-old");
}

/// A pending-verification job outranks apply/prepare when nothing is
/// active.
#[test]
fn hydrate_selection_priority() {
    let mut lifecycle = SchemaDiffJobLifecycle::default();
    lifecycle.hydrate_begin();
    let jobs = parse_job_views(json!([
        view_json("apply-1", "schemaDiffApply", "succeeded", 30),
        { "jobId": "pending-1", "kind": "schemaDiffApply", "state": "succeeded",
          "progress": {}, "updatedAt": 20, "pendingVerificationReason": "outcomeUnknown" },
        view_json("prepare-1", "schemaDiffPrepare", "succeeded", 10),
    ]))
    .unwrap();
    // The pending job (apply kind) is selected; the apply job also reads.
    let effects = lifecycle.on_list_jobs(jobs);
    assert_eq!(
        effects,
        vec![
            LifecycleEffect::GetJobDetails("pending-1".to_owned()),
            LifecycleEffect::GetJobDetails("apply-1".to_owned()),
        ]
    );
    // No active job: after the reads the hydration finishes (no Wait).
    assert!(lifecycle.on_details(0, details("pending-1", "schemaDiffApply", "succeeded")).is_empty());
    assert!(lifecycle.on_details(0, details("apply-1", "schemaDiffApply", "succeeded")).is_empty());
}

/// A failed read sets the error slot and the queue moves on.
#[test]
fn read_error_records_and_advances() {
    let mut lifecycle = SchemaDiffJobLifecycle::default();
    lifecycle.hydrate_begin();
    let jobs = parse_job_views(json!([
        view_json("j1", "schemaDiffPrepare", "running", 10),
        view_json("j2", "schemaDiffApply", "succeeded", 5),
    ]))
    .unwrap();
    let effects = lifecycle.on_list_jobs(jobs);
    assert_eq!(effects.len(), 2);
    assert!(lifecycle.on_read_error("connection refused".to_owned()).is_empty());
    assert_eq!(lifecycle.error(), Some("connection refused"));
    // The queue advanced: the second read still lands, and with j1 active
    // (never read) the poll arms against it.
    let effects = lifecycle.on_details(0, details("j2", "schemaDiffApply", "succeeded"));
    assert!(matches!(effects.as_slice(), [LifecycleEffect::Wait { .. }]));
}

/// `requestCancel`: inactive jobs refuse; active ones request and the
/// answer maps to the outcome.
#[test]
fn cancel_outcomes() {
    let mut lifecycle = SchemaDiffJobLifecycle::default();
    assert!(lifecycle.request_cancel().is_none());

    // A TERMINAL tracked job also refuses (only queued/running may ask).
    lifecycle.accept(parse_job_accepted(json!({
        "jobId": "done-1", "kind": "schemaDiffApply", "state": "succeeded",
        "progress": { "read": 0, "converted": 0, "attempted": 0, "committed": 0, "unknown": 0 }
    })).unwrap());
    assert!(lifecycle.request_cancel().is_none(), "terminal job refuses");

    lifecycle.accept(parse_job_accepted(json!({
        "jobId": "j1", "kind": "schemaDiffApply", "state": "running",
        "progress": { "read": 0, "converted": 0, "attempted": 0, "committed": 0, "unknown": 0 }
    })).unwrap());
    assert_eq!(
        lifecycle.request_cancel(),
        Some(LifecycleEffect::CancelDeploy("j1".to_owned()))
    );
    assert_eq!(lifecycle.cancel_outcome(), SchemaDiffCancelOutcome::Requested);

    lifecycle.on_cancel_acknowledged(true);
    assert_eq!(lifecycle.cancel_outcome(), SchemaDiffCancelOutcome::Requested, "ack keeps requested");
    lifecycle.on_cancel_acknowledged(false);
    assert_eq!(lifecycle.cancel_outcome(), SchemaDiffCancelOutcome::Unavailable);
    lifecycle.on_cancel_error("job not found".to_owned());
    assert_eq!(lifecycle.cancel_outcome(), SchemaDiffCancelOutcome::Unavailable);
    assert_eq!(lifecycle.error(), Some("job not found"));
}

/// `verifyRecovery`: the verifying flag and error slot across success and
/// failure.
#[test]
fn verify_recovery_flow() {
    let mut lifecycle = SchemaDiffJobLifecycle::default();
    lifecycle.on_cancel_error("earlier".to_owned());
    let effect = lifecycle.verify_recovery_begin("j1", "session-1");
    assert_eq!(
        effect,
        LifecycleEffect::VerifyRecovery("j1".to_owned(), "session-1".to_owned())
    );
    assert!(lifecycle.verifying());
    assert!(lifecycle.error().is_none(), "the slot clears on begin");

    lifecycle.on_verify_error("boom".to_owned());
    assert!(!lifecycle.verifying());
    assert_eq!(lifecycle.error(), Some("boom"));

    lifecycle.verify_recovery_begin("j1", "session-1");
    lifecycle.on_verify_details(details("j1", "schemaDiffApply", "failed"));
    assert!(!lifecycle.verifying());
    assert_eq!(lifecycle.current_job().unwrap().job_id, "j1");
    assert!(lifecycle.latest_apply().is_some());
}

/// The wire parse: camelCase fields, the pending-verification reason and
/// the progress counter forms survive.
#[test]
fn list_jobs_wire_parse() {
    let jobs = parse_job_views(json!([
        { "jobId": "a", "kind": "schemaDiffPrepare", "state": "queued",
          "progress": { "attempted": "3" }, "updatedAt": 7, "pendingVerificationReason": null },
        { "jobId": "b", "kind": "schemaDiffApply", "state": "failed",
          "progress": { "committed": 2 }, "updatedAt": 9,
          "pendingVerificationReason": "outcomeUnknown", "cancelRequested": true, "error": "x" },
    ]))
    .unwrap();
    assert_eq!(jobs.len(), 2);
    assert_eq!(jobs[1].state, JobState::Failed);
    assert_eq!(jobs[1].pending_verification_reason.as_deref(), Some("outcomeUnknown"));
    assert_eq!(jobs[1].updated_at, 9);
    assert!(jobs[1].cancel_requested);
    let _ = JobRecoveryVerdictVm::RequireManualReview.wire_str();
}
