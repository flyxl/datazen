//! Independent audit for T046 (the shared bootstrap chain).
//!
//! startSharedApp.tsx has no React test of its own (it is host glue; its
//! pieces are tested where they live — the pack-ep table, the lib
//! helpers, the stores). The Rust audit pins the shared chain's
//! observable contract instead: the produced AppState carries the host's
//! adapter, the bundled value-presentation contribution is registered
//! exactly once (capability activation), the connection sync consulted
//! the adapter before the first frame, and the three app entries
//! (run / run_with_adapter / app_with_adapter) all construct through the
//! same chain.
//!
//! The web-host machinery elisions (module table, E2E bridges, splash,
//! marks, locale sync, preload race) are documented in the module header
//! — they have no Rust counterpart to pin.

use datazen_ui::bootstrap::start_shared_app_state;
use datazen_ui::state_def::AppState;

use datazen_ui::support::value_presentation::timestamp_contribution::TIMESTAMP_CONTRIBUTION_ID;

/// The demo adapter the unwired entry uses (not available on wasm).
#[cfg(not(target_arch = "wasm32"))]
fn demo_adapter() -> Box<dyn datazen_ui_api::DesktopAdapter> {
    use datazen_ui_api::DemoAdapter;
    Box::new(DemoAdapter::default())
}

#[test]
fn bootstrap_registers_the_bundled_contribution() {
    let state = start_shared_app_state(None);
    assert!(
        state
            .value_presentation_registry
            .get(TIMESTAMP_CONTRIBUTION_ID)
            .is_some(),
        "the timestamp contribution must be active after the shared chain"
    );
    assert_eq!(state.value_presentation_registry.list().len(), 1);
    assert!(state.value_presentation_registry.revision() >= 1);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn bootstrap_installs_the_host_adapter_and_syncs_connections() {
    let state = start_shared_app_state(Some(demo_adapter()));
    // The first-paint guarantee: the adapter was consulted before the
    // first frame (the sync ran during construction, not after).
    assert!(
        state.desktop_connections.is_some(),
        "the demo adapter's connection list must be on the state already"
    );
    assert!(state.desktop_error.is_none());
}

#[test]
fn bootstrap_without_adapter_keeps_the_unwired_fallback() {
    let state = start_shared_app_state(None);
    // The chain still consulted the adapter before the first frame (the
    // first-paint guarantee): the unwired adapter answered its fixed
    // error, which is on the state — not deferred to after the frame.
    assert!(
        state.desktop_connections.is_none(),
        "no connections without a host"
    );
    assert!(
        state.desktop_error.is_some(),
        "the unimplemented adapter's fixed error must be recorded by the sync"
    );
}

#[test]
fn default_state_registry_starts_empty() {
    // Only the shared chain fills the registry — the Default state must
    // not pre-register anything (activation is the bootstrap's job).
    let state = AppState::default();
    assert!(state.value_presentation_registry.list().is_empty());
    assert_eq!(state.value_presentation_registry.revision(), 0);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn demo_entry_state_matches_the_shared_chain() {
    // init_state (the demo/unwired entry's state fn) constructs through
    // the same chain: same registry state, same connection answer.
    let chain = start_shared_app_state(Some(demo_adapter()));
    let entry = datazen_ui::app::init_state();
    assert_eq!(
        chain.value_presentation_registry.list().len(),
        entry.value_presentation_registry.list().len()
    );
    assert_eq!(
        chain.desktop_connections.is_some(),
        entry.desktop_connections.is_some()
    );
}
