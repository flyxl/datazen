//! Independent audit for T115 (the SchemaDiffJobStatusPanel Rune port).
//!
//! Tester-written: every assertion comes from re-reading
//! `packages/frontend/src/windows/schema-diff/SchemaDiffJobStatusPanel.tsx`
//! and `__tests__/SchemaDiffJobStatusPanel.test.tsx`: the recovery
//! notices (not-dispatched vs live failure, the pending-verification
//! verify button and its jobId payload), the active job's cancel journey,
//! the plan-unavailable note and the last-apply report line. Element
//! evidence comes from `Runtime::elements()`; presses go through real
//! pointer events.

use serde_json::json;

use rune::{Cx, El, Env, Runtime};

use crate::commands::schema_diff::wire::SchemaDiffJobDetailsVm;
use crate::i18n::t;
use crate::views::data_transfer::data_transfer_attached_jobs::JobState;
use crate::views::schema_diff::schema_diff_job_status_panel::SchemaDiffJobStatusPanel;
use crate::views::schema_diff::use_schema_diff_job_lifecycle::{
    SchemaDiffCancelOutcome, SchemaDiffTrackedJobVm,
};

fn details_json(
    recovery: Option<serde_json::Value>,
    pending_verification_reason: Option<&str>,
    state: &str,
    cancel_requested: bool,
) -> serde_json::Value {
    let mut payload = json!({
        "details": {
            "job": {
                "jobId": "schemaDiffApply-test",
                "kind": "schemaDiffApply",
                "state": state,
                "stage": null,
                "progress": { "read": 0, "converted": 0, "attempted": "3", "committed": "2", "unknown": "1" },
                "updatedAt": 0,
                "cancelRequested": cancel_requested,
                "error": null
            },
            "stateVersion": 2,
            "recoveryTargets": [{ "connectionId": "target-connection", "objectIds": ["obj-1"] }]
        },
        "prepared": null,
        "planUnavailableAfterRestart": false,
        "deployResult": null
    });
    if let Some(pending) = pending_verification_reason {
        payload["details"]["job"]["pendingVerificationReason"] = json!(pending);
    }
    if let Some(recovery) = recovery {
        payload["details"]["recovery"] = recovery;
    }
    payload
}

fn tracked(details: Option<SchemaDiffJobDetailsVm>, state: JobState) -> SchemaDiffTrackedJobVm {
    SchemaDiffTrackedJobVm {
        job_id: "schemaDiffApply-test".to_owned(),
        kind: "schemaDiffApply".to_owned(),
        state,
        progress: Default::default(),
        details,
        submitted_locally: true,
    }
}

fn parse(payload: serde_json::Value) -> SchemaDiffJobDetailsVm {
    crate::commands::schema_diff::normalize_job_details(payload).unwrap()
}

#[derive(Clone)]
struct Scene {
    current_job: Option<SchemaDiffTrackedJobVm>,
    latest_apply: Option<SchemaDiffJobDetailsVm>,
    cancel_outcome: SchemaDiffCancelOutcome,
    verifying: bool,
    target_ready: bool,
    cancels: usize,
    verifies: Vec<String>,
    dark: bool,
}

impl Scene {
    fn with_tracked(details: SchemaDiffJobDetailsVm, state: JobState) -> Self {
        Self {
            current_job: Some(tracked(Some(details), state)),
            latest_apply: None,
            cancel_outcome: SchemaDiffCancelOutcome::Idle,
            verifying: false,
            target_ready: true,
            cancels: 0,
            verifies: Vec::new(),
            dark: true,
        }
    }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    SchemaDiffJobStatusPanel::<Scene>::new(
        s.current_job.as_ref(),
        s.latest_apply.as_ref(),
        s.cancel_outcome,
        s.verifying,
        s.target_ready,
    )
    .on_cancel(|s: &mut Scene| s.cancels += 1)
    .on_verify(|s: &mut Scene, job_id| s.verifies.push(job_id))
    .build(s.dark)
    .unwrap_or_else(|| rune::col().key("schema-diff-job-status-absent"))
}

fn rt(scene: Scene) -> Runtime<Scene> {
    let mut rt = Runtime::new(
        scene,
        view,
        (960.0, 640.0),
        Env { system_dark: true, scale: 1.0 },
    );
    rt.frame(0.0);
    rt
}

fn maybe(rt: &Runtime<Scene>, key: &str) -> Option<rune::ElementInfo> {
    rt.elements()
        .into_iter()
        .find(|e| !e.exiting && e.key.as_deref() == Some(key))
}

fn text_of(rt: &Runtime<Scene>, key: &str) -> String {
    maybe(rt, key).and_then(|e| e.text).unwrap_or_default()
}

/// The first text element inside a keyed element's subtree (kit buttons
/// carry their label on an inner element).
fn text_under(rt: &Runtime<Scene>, key: &str) -> String {
    let elements = rt.elements();
    let index = elements
        .iter()
        .position(|e| !e.exiting && e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"));
    if let Some(text) = &elements[index].text {
        return text.clone();
    }
    let mut frontier = vec![index];
    while let Some(current) = frontier.pop() {
        for (candidate, element) in elements.iter().enumerate() {
            if element.parent == Some(current) && !element.exiting {
                if let Some(text) = &element.text {
                    if !text.is_empty() {
                        return text.clone();
                    }
                }
                frontier.push(candidate);
            }
        }
    }
    String::new()
}

fn press(rt: &mut Runtime<Scene>, key: &str) {
    let point = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_move(point.0, point.1);
    rt.frame(0.0);
    rt.pointer_down(point.0, point.1);
    rt.pointer_up(point.0, point.1);
    rt.frame(0.0);
    rt.frame(0.0);
}

/// The React test: a not-dispatched job explains itself without offering
/// verification.
#[test]
fn not_dispatched_explains_without_verification() {
    let details = parse(details_json(
        Some(json!({ "verdict": "notExecuted", "reasonCode": "notDispatchedAfterRestart" })),
        None,
        "failed",
        false,
    ));
    let runtime = rt(Scene::with_tracked(details, JobState::Failed));
    assert_eq!(
        text_of(&runtime, "schema-diff-job-status-not-dispatched"),
        t("schemaDiff.notDispatchedAfterRestart")
    );
    assert!(maybe(&runtime, "schema-diff-job-status-verify").is_none());
}

/// The React test: a live dispatch failure is not a restart.
#[test]
fn live_dispatch_failure_is_not_a_restart() {
    let details = parse(details_json(
        Some(json!({ "verdict": "notExecuted", "reasonCode": "dispatchFailed" })),
        None,
        "failed",
        false,
    ));
    let runtime = rt(Scene::with_tracked(details, JobState::Failed));
    assert_eq!(
        text_of(&runtime, "schema-diff-job-status-not-executed"),
        t("schemaDiff.notExecuted")
    );
    assert!(maybe(&runtime, "schema-diff-job-status-not-dispatched").is_none());
}

/// The React test: a pending job with target identity offers the
/// read-only verification, and the press carries the jobId.
#[test]
fn pending_verification_offers_verify_with_job_id() {
    let details = parse(details_json(
        Some(json!({ "verdict": "pendingVerification", "reasonCode": "restartNeedsVerification" })),
        Some("restartNeedsVerification"),
        "failed",
        false,
    ));
    let mut runtime = rt(Scene::with_tracked(details, JobState::Failed));
    press(&mut runtime, "schema-diff-job-status-verify");
    assert_eq!(runtime.state.verifies, ["schemaDiffApply-test".to_owned()]);

    // verifying: the label swaps and the button refuses.
    let mut scene = Scene::with_tracked(
        parse(details_json(
            Some(json!({ "verdict": "pendingVerification", "reasonCode": "restartNeedsVerification" })),
            Some("restartNeedsVerification"),
            "failed",
            false,
        )),
        JobState::Failed,
    );
    scene.verifying = true;
    let runtime = rt(scene);
    assert_eq!(
        text_under(&runtime, "schema-diff-job-status-verify"),
        t("schemaDiff.verifyingRecovery")
    );

    // target not ready: same refusal.
    let mut scene = Scene::with_tracked(
        parse(details_json(
            Some(json!({ "verdict": "pendingVerification", "reasonCode": "restartNeedsVerification" })),
            Some("restartNeedsVerification"),
            "failed",
            false,
        )),
        JobState::Failed,
    );
    scene.target_ready = false;
    let mut runtime = rt(scene);
    press(&mut runtime, "schema-diff-job-status-verify");
    assert!(runtime.state.verifies.is_empty(), "disabled while target not ready");
}

/// Without recovery targets the identity is unavailable (no button).
#[test]
fn pending_without_targets_shows_identity_unavailable() {
    let mut payload = details_json(
        Some(json!({ "verdict": "pendingVerification", "reasonCode": "restartNeedsVerification" })),
        Some("restartNeedsVerification"),
        "failed",
        false,
    );
    payload["details"]["recoveryTargets"] = json!([]);
    let runtime = rt(Scene::with_tracked(parse(payload), JobState::Failed));
    assert_eq!(
        text_of(&runtime, "schema-diff-job-status-recovery-identity"),
        t("schemaDiff.recoveryIdentityUnavailable")
    );
    assert!(maybe(&runtime, "schema-diff-job-status-verify").is_none());
}

/// The active job line: kind/state label, stage, progress counters and
/// the cancel button; the cancel-requested note when requested.
#[test]
fn active_job_line_and_cancel_journey() {
    let mut payload = details_json(None, None, "running", true);
    payload["details"]["job"]["stage"] = json!("deploying");
    let mut runtime = rt(Scene::with_tracked(parse(payload), JobState::Running));
    assert_eq!(
        text_of(&runtime, "schema-diff-job-status-kind-state"),
        format!("{}: {}", t("schemaDiff.applyJob"), t("schemaDiff.jobRunning"))
    );
    assert_eq!(text_of(&runtime, "schema-diff-job-status-stage"), "deploying");
    let expected = t("schemaDiff.jobProgress")
        .replace("{attempted}", "3")
        .replace("{committed}", "2")
        .replace("{unknown}", "1");
    assert_eq!(text_of(&runtime, "schema-diff-job-status-progress"), expected);
    assert_eq!(
        text_of(&runtime, "schema-diff-job-status-cancel-requested"),
        t("schemaDiff.cancelRequested")
    );
    press(&mut runtime, "schema-diff-job-status-cancel");
    assert_eq!(runtime.state.cancels, 1);

    // The unavailable outcome note.
    let mut payload = details_json(None, None, "running", false);
    payload["details"]["job"]["stage"] = serde_json::Value::Null;
    let mut scene = Scene::with_tracked(parse(payload), JobState::Running);
    scene.cancel_outcome = SchemaDiffCancelOutcome::Unavailable;
    let runtime = rt(scene);
    assert_eq!(
        text_of(&runtime, "schema-diff-job-status-cancel-unavailable"),
        t("schemaDiff.cancelUnavailable")
    );
}

/// A terminal job shows no cancel button.
#[test]
fn terminal_job_has_no_cancel() {
    let details = parse(details_json(None, None, "succeeded", false));
    let runtime = rt(Scene::with_tracked(details, JobState::Succeeded));
    assert!(maybe(&runtime, "schema-diff-job-status-cancel").is_none());
    assert_eq!(
        text_of(&runtime, "schema-diff-job-status-kind-state"),
        format!("{}: {}", t("schemaDiff.applyJob"), t("schemaDiff.jobSucceeded"))
    );
}

/// The plan-unavailable note and the last-apply report line with its
/// recovery code.
#[test]
fn plan_unavailable_and_last_apply_report() {
    let mut payload = details_json(None, None, "failed", false);
    payload["planUnavailableAfterRestart"] = json!(true);
    let runtime = rt(Scene::with_tracked(parse(payload), JobState::Failed));
    assert_eq!(
        text_of(&runtime, "schema-diff-job-status-plan-unavailable"),
        t("schemaDiff.planUnavailableAfterRestart")
    );

    let mut latest = details_json(None, None, "succeeded", false);
    latest["deployResult"] = json!({
        "status": "mixed", "executedCount": 2, "statementCount": 5,
        "errors": [], "statementResults": []
    });
    latest["details"]["recovery"] = json!({ "verdict": "resumeAfterVerify", "reasonCode": "partialCommit" });
    let mut scene = Scene::with_tracked(parse(details_json(None, None, "failed", false)), JobState::Failed);
    scene.latest_apply = Some(parse(latest));
    let runtime = rt(scene);
    assert_eq!(
        text_of(&runtime, "schema-diff-job-status-last-report"),
        format!("{}: {}", t("schemaDiff.lastApplyReport"), "mixed")
    );
    assert_eq!(
        text_of(&runtime, "schema-diff-job-status-last-counts"),
        format!("2/5 {}", t("schemaDiff.executed"))
    );
    assert_eq!(
        text_of(&runtime, "schema-diff-job-status-recovery-code"),
        t("schemaDiff.recoveryCode").replace("{code}", "partialCommit")
    );
}

/// The recovery-details fallback: when the CURRENT details carry no
/// pending reason but the LATEST APPLY does, the notices read the latest
/// apply (its job id, its targets).
#[test]
fn recovery_fallback_reads_the_latest_apply() {
    // Tracked: failed apply with no recovery and no pending reason.
    let mut scene = Scene::with_tracked(parse(details_json(None, None, "failed", false)), JobState::Failed);
    // Latest apply: a different job, pending verification with targets.
    let mut latest = details_json(
        Some(json!({ "verdict": "pendingVerification", "reasonCode": "restartNeedsVerification" })),
        Some("restartNeedsVerification"),
        "failed",
        false,
    );
    latest["details"]["job"]["jobId"] = json!("apply-older");
    latest["details"]["job"]["kind"] = json!("schemaDiffApply");
    scene.latest_apply = Some(parse(latest));
    let mut runtime = rt(scene);
    // The verify button reads the LATEST apply's identity.
    press(&mut runtime, "schema-diff-job-status-verify");
    assert_eq!(runtime.state.verifies, ["apply-older".to_owned()]);
}

/// Nothing tracked renders nothing (React's null).
#[test]
fn nothing_tracked_renders_nothing() {
    let runtime = rt(Scene {
        current_job: None,
        latest_apply: None,
        cancel_outcome: SchemaDiffCancelOutcome::Idle,
        verifying: false,
        target_ready: true,
        cancels: 0,
        verifies: Vec::new(),
        dark: true,
    });
    assert!(maybe(&runtime, "schema-diff-job-status").is_none());
}
