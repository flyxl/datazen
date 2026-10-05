//! Apply-side plan admission (pure).
//!
//! An apply Job carries **only** planId + digest + selection revision + the reviewed
//! selection; the plan body itself is always read back from the stored plan. Every
//! admission rule below therefore compares what the caller reviewed against what the
//! host actually holds, and fails closed with a "re-prepare" instruction instead of
//! reusing a stale plan (§2.1 / §8).

use crate::commands::error::CommandError;

/// Consumption state of a stored plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlanAvailability {
    /// Issued and not yet handed to any Job.
    Available,
    /// An apply Job already holds it.
    Claimed,
    /// Its apply Job already reached a terminal state.
    Consumed,
}

/// Host-side facts about a stored plan, projected for admission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PlanAdmission {
    pub(crate) plan_id: String,
    pub(crate) plan_digest: String,
    pub(crate) selection_revision: u64,
    pub(crate) expires_at_millis: i64,
    pub(crate) now_millis: i64,
    pub(crate) availability: PlanAvailability,
    pub(crate) can_execute: bool,
    pub(crate) destructive: bool,
}

/// The apply-side facts the caller echoes back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ApplyPlanRequest {
    pub(crate) plan_id: String,
    pub(crate) plan_digest: String,
    pub(crate) selection_revision: u64,
    pub(crate) confirmed_destructive: bool,
}

/// Admit or refuse the apply Job. All rejections tell the caller to prepare again.
pub(crate) fn admit_apply_plan(
    admission: &PlanAdmission,
    request: &ApplyPlanRequest,
) -> Result<(), CommandError> {
    if request.plan_id != admission.plan_id {
        return Err(refuse(
            "apply references a plan that was not issued for this review",
        ));
    }
    // Consumption is checked before expiry/digest so a replayed apply reports the
    // one-shot fact instead of a time-dependent reason (§2.1: one planId ⇒ one Job).
    match admission.availability {
        PlanAvailability::Available => {}
        PlanAvailability::Claimed => {
            return Err(refuse(
                "plan is already held by an apply Job; one planId yields one Job",
            ))
        }
        PlanAvailability::Consumed => {
            return Err(refuse(
                "plan was already consumed; re-prepare to review again",
            ))
        }
    }
    if admission.expires_at_millis <= admission.now_millis {
        return Err(refuse("plan expired; re-prepare to review again"));
    }
    if request.plan_digest != admission.plan_digest {
        return Err(refuse(
            "plan digest changed since review; re-prepare to review again",
        ));
    }
    if request.selection_revision != admission.selection_revision {
        return Err(refuse(
            "selection revision changed since review; re-prepare to review again",
        ));
    }
    if !admission.can_execute {
        return Err(refuse(
            "plan is not executable in its current context; re-prepare",
        ));
    }
    if admission.destructive && !request.confirmed_destructive {
        return Err(CommandError::Validation(
            "destructive write requires explicit confirmation (confirmedDestructive = true)"
                .to_string(),
        ));
    }
    Ok(())
}

fn refuse(reason: &str) -> CommandError {
    CommandError::Validation(reason.to_string())
}
