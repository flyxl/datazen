use super::*;
use super::counters::{is_migration_job_in_flight, read_job_counter, sum_job_counters};

fn num(value: f64) -> JobCounterValue {
    JobCounterValue::Number(value)
}

fn text(value: &str) -> JobCounterValue {
    JobCounterValue::Text(value.to_owned())
}

#[test]
fn counters_accept_both_wire_forms() {
    assert_eq!(read_job_counter(&num(42.0)), Some(42));
    assert_eq!(read_job_counter(&num(3.9)), Some(3));
    assert_eq!(read_job_counter(&num(-1.0)), None);
    assert_eq!(read_job_counter(&num(f64::NAN)), None);
    assert_eq!(read_job_counter(&num(f64::INFINITY)), None);
    assert_eq!(read_job_counter(&text("42")), Some(42));
    assert_eq!(read_job_counter(&text("  00012 ")), Some(12));
    assert_eq!(read_job_counter(&text("4.2")), None);
    assert_eq!(read_job_counter(&text("")), None);
    assert_eq!(read_job_counter(&text("9007199254740992")), None);
    assert_eq!(read_job_counter(&JobCounterValue::Null), None);
}

#[test]
fn counter_sums_fail_closed() {
    assert_eq!(sum_job_counters(&[num(1.0), text("2"), num(3.5)]), Some(6));
    assert_eq!(sum_job_counters(&[]), Some(0));
    assert_eq!(sum_job_counters(&[num(1.0), JobCounterValue::Null]), None);
    assert_eq!(sum_job_counters(&[num(1.0), text("bad")]), None);
}

#[test]
fn in_flight_is_only_queued_or_running() {
    assert!(is_migration_job_in_flight("queued"));
    assert!(is_migration_job_in_flight("running"));
    assert!(!is_migration_job_in_flight("succeeded"));
    assert!(!is_migration_job_in_flight("failed"));
}

#[test]
fn cancel_disposition_distinguishes_the_races() {
    assert_eq!(derive_cancel_disposition(None, false, 0, false), CancelDisposition::None);
    assert_eq!(derive_cancel_disposition(None, true, 0, true), CancelDisposition::RequestedInFlight);
    assert_eq!(derive_cancel_disposition(Some("completed"), true, 1, false), CancelDisposition::SettledCompleted);
    assert_eq!(derive_cancel_disposition(Some("rolledBack"), true, 0, false), CancelDisposition::SettledRolledBack);
    assert_eq!(derive_cancel_disposition(Some("partiallyApplied"), true, 0, false), CancelDisposition::SettledPartially);
    assert_eq!(derive_cancel_disposition(Some("unknown"), true, 0, false), CancelDisposition::SettledUnknown);
    assert_eq!(derive_cancel_disposition(None, true, 0, false), CancelDisposition::NotStarted);
    assert_eq!(derive_cancel_disposition(None, true, 1, false), CancelDisposition::SettledPartially);
    assert_eq!(derive_cancel_disposition(Some("notStarted"), true, 0, false), CancelDisposition::NotStarted);
}

#[test]
fn uncertainty_fails_closed_on_the_explicit_conditions() {
    assert_eq!(derive_uncertainty(Some("unknown"), None, None, 0, false, false), UncertaintyReason::EffectOutcomeUnknown);
    assert_eq!(derive_uncertainty(None, Some("requireManualReview"), None, 0, false, false), UncertaintyReason::ManualReviewRequired);
    assert_eq!(derive_uncertainty(None, Some("reject"), None, 0, false, false), UncertaintyReason::RecoveryRejected);
    assert_eq!(derive_uncertainty(None, Some("bogus"), None, 0, false, false), UncertaintyReason::RecoveryRejected);
    assert_eq!(derive_uncertainty(Some("completed"), Some("resumeAfterVerify"), None, 0, false, false), UncertaintyReason::None);
    assert_eq!(derive_uncertainty(None, None, Some("No checkpoint was recorded"), 0, false, false), UncertaintyReason::NoCheckpoint);
    assert_eq!(derive_uncertainty(None, None, None, 0, true, false), UncertaintyReason::MissingEvidence);
    assert_eq!(derive_uncertainty(None, None, None, 1, false, false), UncertaintyReason::MissingEvidence);
    assert_eq!(derive_uncertainty(None, None, None, 0, false, false), UncertaintyReason::None);
}

#[test]
fn not_executed_needs_a_clear_receipt_and_no_write_evidence() {
    assert_eq!(
        derive_uncertainty(Some("notStarted"), Some("notExecuted"), Some("notDispatchedAfterRestart"), 0, false, false),
        UncertaintyReason::None
    );
    assert_eq!(
        derive_uncertainty(Some("notStarted"), Some("notExecuted"), Some("notDispatchedAfterRestart"), 0, false, true),
        UncertaintyReason::RecoveryRejected
    );
    assert_eq!(
        derive_uncertainty(Some("rolledBack"), Some("notExecuted"), Some("noCommittedWrites"), 0, false, false),
        UncertaintyReason::None
    );
    assert_eq!(
        derive_uncertainty(Some("completed"), Some("notExecuted"), Some("noCommittedWrites"), 0, false, false),
        UncertaintyReason::RecoveryRejected
    );
}

#[test]
fn a_clean_completed_write_is_ok_and_settled() {
    let input = MigrationJobVerdictInput {
        state: "succeeded".to_owned(),
        effect_outcome: Some("completed".to_owned()),
        commit_boundaries: Some(vec![CommitBoundaryFacts { evidence: vec!["EVIDENCE_OK".to_owned()] }]),
        ..Default::default()
    };
    let verdict = derive_migration_job_verdict(&input);
    assert_eq!(verdict.severity, VerdictSeverity::Ok);
    assert_eq!(verdict.cancel_disposition, CancelDisposition::None);
    assert!(verdict.completed);
    assert_eq!(verdict.verified_boundaries, 1);
    assert_eq!(verdict.unverified_boundaries, 0);
    assert_eq!(verdict.uncertainty, UncertaintyReason::None);
    assert!(!verdict.requires_reconcile);
}

#[test]
fn committed_rows_without_boundaries_stay_uncertain() {
    let input = MigrationJobVerdictInput {
        state: "succeeded".to_owned(),
        effect_outcome: Some("completed".to_owned()),
        committed_rows: Some(5),
        ..Default::default()
    };
    let verdict = derive_migration_job_verdict(&input);
    assert_eq!(verdict.uncertainty, UncertaintyReason::MissingEvidence);
    assert_eq!(verdict.severity, VerdictSeverity::Uncertain);
    assert!(!verdict.completed);
    assert!(verdict.requires_reconcile);
    assert_eq!(verdict.verified_boundaries, 0);
}

#[test]
fn unverified_boundaries_are_counted_and_raise_uncertainty() {
    let input = MigrationJobVerdictInput {
        state: "succeeded".to_owned(),
        effect_outcome: Some("completed".to_owned()),
        commit_boundaries: Some(vec![
            CommitBoundaryFacts { evidence: vec!["EVIDENCE_OK".to_owned()] },
            CommitBoundaryFacts { evidence: Vec::new() },
        ]),
        ..Default::default()
    };
    let verdict = derive_migration_job_verdict(&input);
    assert_eq!(verdict.verified_boundaries, 1);
    assert_eq!(verdict.unverified_boundaries, 1);
    assert_eq!(verdict.uncertainty, UncertaintyReason::MissingEvidence);
    assert!(!verdict.completed);
    assert!(verdict.requires_reconcile);
}

#[test]
fn in_flight_jobs_are_partial_and_need_reconcile() {
    let input = MigrationJobVerdictInput {
        state: "running".to_owned(),
        effect_outcome: Some("completed".to_owned()),
        cancel_requested: true,
        ..Default::default()
    };
    let verdict = derive_migration_job_verdict(&input);
    assert_eq!(verdict.severity, VerdictSeverity::Partial);
    assert_eq!(verdict.cancel_disposition, CancelDisposition::RequestedInFlight);
    assert!(!verdict.completed);
    assert!(verdict.requires_reconcile);
}

#[test]
fn failed_state_and_error_produce_failed_severity() {
    let input = MigrationJobVerdictInput {
        state: "failed".to_owned(),
        effect_outcome: Some("rolledBack".to_owned()),
        error: Some("boom".to_owned()),
        ..Default::default()
    };
    let verdict = derive_migration_job_verdict(&input);
    assert_eq!(verdict.severity, VerdictSeverity::Failed);
    assert!(!verdict.completed);
}

#[test]
fn settled_unknown_needs_reconcile_even_when_severity_is_uncertain() {
    let input = MigrationJobVerdictInput {
        state: "succeeded".to_owned(),
        effect_outcome: Some("unknown".to_owned()),
        cancel_requested: true,
        ..Default::default()
    };
    let verdict = derive_migration_job_verdict(&input);
    assert_eq!(verdict.cancel_disposition, CancelDisposition::SettledUnknown);
    assert_eq!(verdict.severity, VerdictSeverity::Uncertain);
    assert!(verdict.requires_reconcile);
}

#[test]
fn pipeline_budget_constant_and_interception_patterns() {
    assert_eq!(PIPELINE_BUDGET_BYTES, 8 * 1024 * 1024);
    assert!(!is_pipeline_budget_interception(None));
    assert!(!is_pipeline_budget_interception(Some("unrelated")));
    for message in [
        "Pipeline budget exceeded",
        "pipelinebudget",
        "budget   exceed",
        "8 MiB limit",
        "8MiB",
        "8388608 bytes",
    ] {
        assert!(is_pipeline_budget_interception(Some(message)), "{message}");
    }
}

#[test]
fn plan_consumed_and_backend_scope_rejections() {
    assert!(!is_plan_consumed_rejection(None));
    assert!(!is_plan_consumed_rejection(Some("already used")));
    for message in [
        "Already  consumed",
        "PlanAlreadyConsumed",
        "re-review the migration to mint a new plan",
    ] {
        assert!(is_plan_consumed_rejection(Some(message)), "{message}");
    }
    assert!(!is_backend_scope_rejection(None));
    assert!(!is_backend_scope_rejection(Some("unrelated")));
    for message in ["backend   scope", "local-desktop-backend", "scope", "SCOPE"] {
        assert!(is_backend_scope_rejection(Some(message)), "{message}");
    }
}

#[test]
fn crate_root_surface_is_reexported() {
    assert_eq!(crate::PIPELINE_BUDGET_BYTES, 8 * 1024 * 1024);
    let _ = crate::read_job_counter;
    let _ = crate::sum_job_counters;
    let _ = crate::is_migration_job_in_flight;
    let _ = crate::derive_cancel_disposition;
    let _ = crate::derive_uncertainty;
    let _ = crate::derive_migration_job_verdict;
    let _ = crate::is_pipeline_budget_interception;
    let _ = crate::is_plan_consumed_rejection;
    let _ = crate::is_backend_scope_rejection;
    assert_eq!(crate::CancelDisposition::SettledUnknown.as_str(), "settledUnknown");
    assert_eq!(crate::VerdictSeverity::Uncertain.as_str(), "uncertain");
    assert_eq!(crate::UncertaintyReason::NoCheckpoint.as_str(), "noCheckpoint");
    let _: Option<crate::MigrationJobVerdict> = None;
    let _: Option<crate::MigrationJobVerdictInput> = None;
    let _: Option<crate::CommitBoundaryFacts> = None;
    let _: Option<crate::JobCounterValue> = None;
}
