//! `onboardingGate.ts`'s persisted-payload half (T268): the versioned
//! onboarding state written into the settings store, plus the journey
//! revision const.
//!
//! The shape is the shared settings contract's `onboarding` key — the same
//! JSON the backend domain type (`crates/datazen/src/settings/domain.rs`'s
//! `OnboardingState`, camelCase `{ "completed", "version" }`) reads and
//! writes, and the exact object React's
//! `updateSettings({ onboarding: completedOnboardingState() })` merges in.
//! The UI crate keeps its own mirror of the interface (it does not depend
//! on the backend crate); the serde tests below pin the two shapes
//! together.

use serde::{Deserialize, Serialize};

/// Revision of the first-run journey. Bump only to re-onboard every user.
///
/// React: `export const ONBOARDING_VERSION = 1` — a JS `number`; the shared
/// settings contract types the field `i32`, and this port follows the
/// contract.
pub const ONBOARDING_VERSION: i32 = 1;

/// React `OnboardingState` (`onboardingGate.ts:19`): the persisted
/// first-run-journey state.
///
/// The backend owns the fresh-install-vs-upgrade decision (`Store::load_all`):
/// a fresh install materializes `{ completed: false }` (journey shown); an
/// install whose `settings.json` predates the journey resolves to
/// `{ completed: true }` (upgrade ≠ first run). The `version` field records
/// which journey revision the user last saw — bumping
/// [`ONBOARDING_VERSION`] re-onboards everyone.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OnboardingState {
    /// True once the user has left the journey (finished or skipped).
    pub completed: bool,
    /// The journey revision this state was written by.
    pub version: i32,
}

/// React `completedOnboardingState()` (`onboardingGate.ts:31`): the settings
/// payload written when the user leaves the journey — the done step's open
/// handler merges it via `updateSettings({ onboarding: … })` before the
/// window's completion action fires (the T114 `on_open` seam's feed-in).
pub fn completed_onboarding_state() -> OnboardingState {
    OnboardingState { completed: true, version: ONBOARDING_VERSION }
}

#[doc(hidden)]  // exposed for integration tests
pub mod tests {
    use super::*;

    // --- completedOnboardingState (onboardingGate.test.ts:25) ----------------

    #[test]
    fn completed_state_stamps_the_current_journey_revision() {
        assert_eq!(
            completed_onboarding_state(),
            OnboardingState { completed: true, version: ONBOARDING_VERSION }
        );
    }

    /// The payload's JSON must be the shared settings contract's exact
    /// `onboarding` shape — the same camelCase object the backend domain
    /// type serializes, so `updateSettings({ onboarding: … })` writes what
    /// `Store::load_all` reads back.
    #[test]
    fn completed_payload_serializes_to_the_settings_contract_shape() {
        let json = serde_json::to_string(&completed_onboarding_state()).unwrap();
        assert_eq!(json, r#"{"completed":true,"version":1}"#);
        assert_eq!(
            serde_json::from_str::<OnboardingState>(&json).unwrap(),
            completed_onboarding_state()
        );
    }

    /// The backend's fresh-install materialization (`{ completed: false }`)
    /// loads verbatim — the state the gate then shows the journey for.
    #[test]
    fn fresh_install_materialization_loads_verbatim() {
        let state: OnboardingState =
            serde_json::from_str(r#"{"completed":false,"version":1}"#).unwrap();
        assert_eq!(state, OnboardingState { completed: false, version: ONBOARDING_VERSION });
    }
}
