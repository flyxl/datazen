use super::cancel::derive_cancel_disposition;
use super::counters::is_migration_job_in_flight;
use super::types::{
    CancelDisposition, MigrationJobVerdict, MigrationJobVerdictInput, UncertaintyReason,
    VerdictSeverity,
};
use super::uncertainty::derive_uncertainty;

fn pick_severity(
    in_flight: bool,
    uncertainty: UncertaintyReason,
    completed: bool,
    state: &str,
    has_error: bool,
    cancel_disposition: CancelDisposition,
) -> VerdictSeverity {
    if uncertainty != UncertaintyReason::None {
        return VerdictSeverity::Uncertain;
    }
    if completed {
        return VerdictSeverity::Ok;
    }
    if in_flight {
        return VerdictSeverity::Partial;
    }
    if has_error || state == "failed" {
        return VerdictSeverity::Failed;
    }
    match cancel_disposition {
        CancelDisposition::NotStarted | CancelDisposition::SettledRolledBack => VerdictSeverity::Failed,
        CancelDisposition::SettledCompleted => VerdictSeverity::Ok,
        CancelDisposition::SettledPartially
        | CancelDisposition::SettledUnknown
        | CancelDisposition::RequestedInFlight
        | CancelDisposition::None => VerdictSeverity::Partial,
    }
}

pub fn derive_migration_job_verdict(input: &MigrationJobVerdictInput) -> MigrationJobVerdict {
    let boundaries = input.commit_boundaries.as_deref().unwrap_or(&[]);
    let verified_boundaries = boundaries.iter().filter(|item| !item.evidence.is_empty()).count();
    let unverified_boundaries = boundaries.len() - verified_boundaries;
    let in_flight = is_migration_job_in_flight(&input.state);
    let cancel_disposition = derive_cancel_disposition(
        input.effect_outcome.as_deref(),
        input.cancel_requested,
        boundaries.len(),
        in_flight,
    );
    let committed = input.committed_rows.unwrap_or(0).max(0) as u128;
    let unknown = input.unknown_rows.unwrap_or(0).max(0) as u128;
    let has_unbacked_commit = boundaries.is_empty() && (committed > 0 || unknown > 0);
    let has_write_evidence = !boundaries.is_empty() || committed > 0 || unknown > 0;
    let uncertainty = derive_uncertainty(
        input.effect_outcome.as_deref(),
        input.recovery_verdict.as_deref(),
        input.recovery_reason.as_deref(),
        unverified_boundaries,
        has_unbacked_commit,
        has_write_evidence,
    );
    let completed = input.effect_outcome.as_deref() == Some("completed")
        && unverified_boundaries == 0
        && !in_flight
        && uncertainty == UncertaintyReason::None;
    let severity = pick_severity(
        in_flight,
        uncertainty,
        completed,
        &input.state,
        input.error.as_deref().is_some_and(|text| !text.is_empty()),
        cancel_disposition,
    );
    MigrationJobVerdict {
        severity,
        cancel_disposition,
        completed,
        verified_boundaries,
        unverified_boundaries,
        uncertainty,
        requires_reconcile: in_flight
            || uncertainty != UncertaintyReason::None
            || cancel_disposition == CancelDisposition::SettledUnknown,
    }
}
