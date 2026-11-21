use super::types::UncertaintyReason;

fn contains_no_checkpoint(reason: &str) -> bool {
    reason.to_lowercase().contains("no checkpoint")
}

pub fn derive_uncertainty(
    effect_outcome: Option<&str>,
    recovery_verdict: Option<&str>,
    recovery_reason: Option<&str>,
    unverified_boundaries: usize,
    has_unbacked_commit: bool,
    has_write_evidence: bool,
) -> UncertaintyReason {
    if effect_outcome == Some("unknown") {
        return UncertaintyReason::EffectOutcomeUnknown;
    }
    if recovery_verdict == Some("requireManualReview") {
        return UncertaintyReason::ManualReviewRequired;
    }
    if recovery_verdict == Some("reject") {
        return UncertaintyReason::RecoveryRejected;
    }
    if recovery_verdict == Some("notExecuted") {
        let restart_before_dispatch =
            effect_outcome == Some("notStarted") && recovery_reason == Some("notDispatchedAfterRestart");
        let no_committed_writes = matches!(effect_outcome, Some("notStarted" | "rolledBack"))
            && recovery_reason == Some("noCommittedWrites");
        if (!restart_before_dispatch && !no_committed_writes) || has_write_evidence {
            return UncertaintyReason::RecoveryRejected;
        }
    }
    if let Some(verdict) = recovery_verdict {
        if verdict != "resumeAfterVerify" && verdict != "notExecuted" {
            return UncertaintyReason::RecoveryRejected;
        }
    }
    if let Some(reason) = recovery_reason {
        if contains_no_checkpoint(reason) {
            return UncertaintyReason::NoCheckpoint;
        }
    }
    if has_unbacked_commit || unverified_boundaries > 0 {
        return UncertaintyReason::MissingEvidence;
    }
    UncertaintyReason::None
}
