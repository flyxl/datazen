//! Port of `packages/frontend/src/lib/valuePresentation/registerBundledContributions.ts`
//! (issue-32 T06) — the startup registration point for the core built-in
//! presentation contributions.
//!
//! The T05 production gap this closed: nothing registered the timestamp
//! contribution, so the default snapshot stayed empty and temporal
//! columns always rendered with the default display. Activation only
//! registers capabilities (design §6): no Store, file or network reads —
//! configuration loading and persistence belong to the host (the
//! settings store / snapshot hook).
//!
//! Idempotent: an already-registered contribution is left alone (the
//! registry rejects duplicate ids, and startup paths must be safe to
//! re-run). React's global-singleton default parameter becomes the
//! explicit registry argument the Rust host owns.

use crate::support::value_presentation::registry::ValuePresentationRegistry;
use crate::support::value_presentation::timestamp_contribution::{
    TIMESTAMP_CONTRIBUTION_ID, timestamp_contribution,
};

/// Register every core built-in presentation contribution (currently
/// only `datazen.timestamp`). Idempotent and never unregistered.
pub(crate) fn register_bundled_value_presentation_contributions(
    registry: &mut ValuePresentationRegistry,
) {
    if registry.get(TIMESTAMP_CONTRIBUTION_ID).is_some() {
        return;
    }
    let _ = registry.register(timestamp_contribution());
}
