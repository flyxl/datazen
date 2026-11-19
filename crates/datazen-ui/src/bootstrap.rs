//! Port of `packages/frontend/src/bootstrap/startSharedApp.tsx` (host
//! boundary follow-up F2; T046).
//!
//! The React bootstrap pulled the shared startup chain out of the desktop
//! entry so a future web host could call the same function. The Rust
//! mirror is the state-side chain both native entries share — the demo
//! bin (`run`), the adapter-driven runs (`run_with_adapter`) and the
//! live host's `app_with_adapter` all construct their first `AppState`
//! through [`start_shared_app_state`]:
//!
//! 1. the adapter seam (the host's use-case bundle; the demo/unwired
//!    fallback when none is given),
//! 2. `registerBundledValuePresentationContributions` (issue-32 T06): the
//!    core built-in presentation contributions activate once at startup —
//!    capability registration only, configuration loading stays with the
//!    settings layer; never unregistered,
//! 3. the connection sync — the Rust first-paint guarantee: the
//!    connections page consults the adapter before the first frame, so
//!    its answer (or its fixed error) is on the page from the start.
//!
//! # Known elisions (web-host machinery with no Rust counterpart)
//!
//! - The `__DATAZEN_HOST__` module table exists so a blob-URL-loaded PRO
//!   extension can resolve bare specifiers inside a JS realm. The Rust
//!   host has no JS realm and no runtime-loaded extensions — extensions
//!   are compile-time contributions registered into the registry.
//! - The E2E store bridges (`__schemaStore`, `__tableDataStore`, …) are
//!   WebDriver/Vite-build hooks; the Rust tests exercise state directly.
//! - Startup marks, the splash and the 2×rAF first-paint mark: the rune
//!   window system paints; there is no splash to hide or frame callback
//!   to schedule. The startup-timer infrastructure belongs to the web
//!   host's performance tooling.
//! - `startLocaleSync`: the Rust i18n is a compile-time table; the
//!   settings-driven locale switch lands with the settings hooks.
//! - The 3s settings preload race: the live host opens its repository
//!   *before* app construction (its `main`), so the shared chain receives
//!   an already-bound adapter — the timeout/continue shape is the web
//!   host's async-preload concern.
//! - `IS_DESKTOP_BUILD` probing: Rust host differences are compile-time
//!   (`cfg`) by construction; each bin binds what it has.

use crate::state_def::AppState;
use datazen_ui_api::DesktopAdapter;
use crate::support::value_presentation::register_bundled_contributions::
    register_bundled_value_presentation_contributions;
use crate::support::value_presentation::registry::ValuePresentationRegistry;
use crate::support::value_presentation::timestamp_contribution::TIMESTAMP_CONTRIBUTION_ID;

/// The shared startup chain's state product: see the module header. The
/// host bins pass their adapter; the demo passes `None` for the
/// unwired/demo fallback.
pub fn start_shared_app_state(adapter: Option<Box<dyn DesktopAdapter>>) -> AppState {
    let mut state = AppState::default();
    if let Some(adapter) = adapter {
        state.desktop = adapter;
    }
    // registerBundledValuePresentationContributions(): activation only
    // registers capability (design §6); the settings layer loads config.
    register_bundled_value_presentation_contributions(&mut state.value_presentation_registry);
    // The first-paint guarantee: the adapter is consulted before the
    // first frame, not after.
    state.sync_desktop_connections();
    state
}

/// The registry the shared chain installed (testing/inspection seam).
pub fn bundled_registry_snapshot(state: &AppState) -> Option<std::rc::Rc<
    dyn crate::support::value_presentation::contracts::ValuePresentationContribution,
>> {
    state.value_presentation_registry.get(TIMESTAMP_CONTRIBUTION_ID)
}

/// Keep a reference alive for the Default-registry path (documentation:
/// the state's registry starts empty; only the shared chain fills it).
#[allow(dead_code)]
fn empty_registry() -> ValuePresentationRegistry {
    ValuePresentationRegistry::new()
}
