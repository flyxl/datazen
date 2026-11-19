//! T049: the `@datazen/frontend/app` barrel (`packages/frontend/src/app.ts`).
//!
//! React's `app.ts` is one re-export line: `startSharedApp` and
//! `StartSharedAppOptions` from `./bootstrap/startSharedApp`, kept out of the
//! light `.` entry so a light consumer's test graph never has to know the app
//! graph exists. In Rune the barrel dissolves into the crate-root re-exports
//! of the launch surface (`run`, `run_with_adapter`, `app_with_adapter`);
//! `startSharedApp` itself has no Rune equivalent yet — that is T047's
//! bootstrap port, not this slice.
//!
//! These tests pin the barrel contract from the consumer's side, headless
//! (no window is ever opened: `App::new` is pure construction, `run` is
//! never called):
//!
//! 1. the launch surface is reachable at the crate root with the exact
//!    signatures a host entry needs;
//! 2. the host entry builds with a host adapter without touching a window;
//! 3. the bootstrap's first-frame state already consulted the adapter
//!    (the demo host's connections are listed) and starts on the
//!    connections page — the observable half of "the adapter is consulted
//!    before the first frame" (`app.rs`).

use datazen_ui_api::{DesktopAdapter, UnimplementedAdapter};
use rune::App;

use crate::{AppState, Section};

/// The barrel's surface, pinned as function pointers: a host entry that
/// silently changed shape (or vanished from the crate root) fails here, not
/// in a host that imports it.
#[test]
fn launch_surface_is_reexported_at_crate_root() {
    let run: fn() = crate::run;
    let app_with_adapter: fn(Box<dyn DesktopAdapter>) -> App<AppState> =
        crate::app_with_adapter;
    // `run_with_adapter` is the live host's entry (native only, like the
    // desktop `startSharedApp` call site); the wasm host uses `run`/`start`.
    #[cfg(not(target_arch = "wasm32"))]
    let run_with_adapter: fn(Box<dyn DesktopAdapter>) = crate::run_with_adapter;
    #[cfg(not(target_arch = "wasm32"))]
    let _ = run_with_adapter;
    let _ = (run, app_with_adapter);
}

/// The host entry builds with the host's adapter and constructs no window:
/// `App::new` only stores the spec, so this stays headless.
#[test]
fn app_with_adapter_builds_without_a_window() {
    let _app: App<AppState> = crate::app_with_adapter(Box::new(UnimplementedAdapter));
}

/// The bootstrap's initial state already consulted the adapter before the
/// first frame: the demo host's two connections are listed, no error is
/// recorded, and the app starts on the connections page.
#[test]
fn init_state_consults_adapter_before_first_frame() {
    let state = crate::app::init_state();
    assert_eq!(state.active, Section::Connections);
    assert_eq!(state.desktop_error, None);
    let connections = state
        .desktop_connections
        .expect("init_state lists the adapter's connections before the first frame");
    let ids: Vec<&str> = connections.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, ["local", "replica"]);
}
