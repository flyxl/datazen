//! `onboardingGate.ts`'s decision half (T268): should the first-run journey
//! replace the main workspace?

use super::state::OnboardingState;

/// React `shouldShowOnboarding` (`onboardingGate.ts:25`): true when the
/// first-run journey should replace the main workspace.
///
/// The journey renders only for an explicitly unfinished state — a fresh
/// install, which the backend materializes as `{ completed: false }`. A
/// missing state is the upgrading-user fallback and must NOT show the
/// journey: gating on `completed` alone (missing → not completed → show)
/// would re-onboard every existing user once — the exact regression the
/// missing-state guard exists for.
///
/// The gate never consults `version` — re-onboarding after a journey
/// revision bump is a backend migration concern (the persisted version only
/// records which revision the user last saw).
pub(crate) fn should_show_onboarding(state: Option<&OnboardingState>) -> bool {
    let Some(state) = state else { return false };
    !state.completed
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use super::super::state::{ONBOARDING_VERSION, completed_onboarding_state};

    // --- shouldShowOnboarding (onboardingGate.test.ts:8) ----------------------

    #[test]
    fn shows_the_journey_only_for_an_explicitly_unfinished_state() {
        // Fresh install: the backend's materialized `{ completed: false }`.
        let fresh = OnboardingState { completed: false, version: ONBOARDING_VERSION };
        assert!(should_show_onboarding(Some(&fresh)));
    }

    #[test]
    fn never_shows_the_journey_for_upgrading_users() {
        // Legacy settings.json without the key — the backend marks these as
        // completed, but a missing state must also never onboard anyone.
        // (React passes both `undefined` and `null`; both are `None` here.)
        assert!(!should_show_onboarding(None));
    }

    #[test]
    fn hides_the_journey_once_it_was_completed() {
        assert!(!should_show_onboarding(Some(&completed_onboarding_state())));
    }

    // --- beyond the React suite: the gate's own edges -------------------------

    /// `version` is never consulted — a completed state written by an older
    /// journey revision still hides the wizard (the re-onboard decision
    /// belongs to the backend, not this gate).
    #[test]
    fn a_completed_state_from_an_older_revision_still_hides_the_journey() {
        let stale = OnboardingState { completed: true, version: ONBOARDING_VERSION - 1 };
        assert!(!should_show_onboarding(Some(&stale)));
    }

    /// An unfinished state shows the journey whatever the version says —
    /// `shouldShowOnboarding({ completed: false })` is true at every
    /// revision, including a not-yet-written (`version: 0`) one.
    #[test]
    fn an_unfinished_state_shows_the_journey_at_any_version() {
        let next = OnboardingState { completed: false, version: ONBOARDING_VERSION + 1 };
        assert!(should_show_onboarding(Some(&next)));
        let unversioned = OnboardingState { completed: false, version: 0 };
        assert!(should_show_onboarding(Some(&unversioned)));
    }
}
