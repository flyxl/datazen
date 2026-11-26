//! T049 audit: the `@datazen/frontend/app` barrel (`packages/frontend/src/app.ts`).
//!
//! The coder's three tests pin the crate-root launch surface (test 1),
//! headless construction with an unwired adapter (test 2), and the *demo*
//! path's first-frame state via `init_state()` (test 3). Test 3's gap: it
//! calls `init_state()` directly, which on native always installs the demo
//! `DemoAdapter::default()` — it never exercises the host
//! half the barrel's doc claims (`configureHost` → the host builds a
//! `DesktopAdapter` and hands it to `run_with_adapter` /
//! `app_with_adapter`, whose `.init` closure then assigns it and calls
//! `sync_desktop_connections()`). These audit tests pin that host half
//! directly, with a non-demo adapter, through the exact assign + sync
//! mechanism the launch closure performs:
//!
//! 1. a host-supplied adapter's list replaces the demo list before the
//!    first frame (not just the demo adapter listing its own seeds);
//! 2. an unwired adapter records its error instead of a list — proving the
//!    sync really consults the adapter rather than returning a canned list;
//! 3. the launch surface accepts a wired host adapter at construction
//!    (the `configureHost` half's type shape), still without a window.

use datazen_ui_api::{ConnectionProfile, DemoAdapter, DESKTOP_NOT_IMPLEMENTED};

use datazen_ui::{AppState, Section};

/// The host half of the barrel: a host-built adapter's answer — not the
/// demo seeds — is what the first frame shows, via the same assign + sync
/// the `app_with_adapter` init closure performs.
#[test]
fn host_adapter_list_is_what_the_first_frame_shows() {
    let mut state = AppState::default();
    state.desktop = Box::new(DemoAdapter::with_connections(vec![ConnectionProfile {
        id: "audit-one".into(),
        name: "Audit One".into(),
        engine: "postgres".into(),
        host: None,
    }]));
    state.sync_desktop_connections();

    assert_eq!(state.active, Section::Connections);
    assert_eq!(state.desktop_error, None);
    let connections = state
        .desktop_connections
        .expect("a wired host adapter's list is recorded before the first frame");
    let ids: Vec<&str> = connections.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, ["audit-one"]);
}

/// The sync really consults the adapter: the default unwired adapter
/// records its fixed error and no list, instead of falling back to demo
/// connections.
#[test]
fn unwired_adapter_records_its_error_not_a_list() {
    let mut state = AppState::default();
    state.sync_desktop_connections();

    assert_eq!(state.desktop_connections, None);
    assert_eq!(
        state.desktop_error.as_deref(),
        Some(DESKTOP_NOT_IMPLEMENTED)
    );
}

/// The launch surface accepts a wired host adapter at construction — the
/// `configureHost` half's shape — and still opens no window: `App::new`
/// only stores the spec.
#[test]
fn app_with_adapter_accepts_a_wired_host_adapter() {
    use rune::App;

    let _app: App<AppState> =
        datazen_ui::app_with_adapter(Box::new(DemoAdapter::with_connections(vec![
            ConnectionProfile {
                id: "audit-one".into(),
                name: "Audit One".into(),
                engine: "postgres".into(),
                host: None,
            },
        ])));
}
