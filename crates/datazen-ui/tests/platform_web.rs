//! T045 — `packages/frontend/src/platform/web/index.ts` (14 lines) parity.
//!
//! React's `platform/web/index.ts` is a *pure re-export barrel*: its only
//! statement is `export { ... } from './stub'` for five web-stub factories
//! (`assembleWebPlatformStub`, `createWebBackendTransportStub`,
//! `createWebEventChannel`, `createWebPlatformServices`,
//! `createWebWindowRouter`). It defines no logic of its own — the real
//! throw / inert behavior lives in `platform/web/stub.ts`, which is its own
//! migration task. A barrel has no Rune body to port: creating an empty
//! `platform/web.rs` would be a fake file. Instead this module records the
//! Rune-side adaptation that already fills the same "one platform assembly,
//! web included" role:
//!
//!   * React's five-stub assembly (`assembleWebPlatformStub`) → Rune's single
//!     cross-platform `run()` entry; there is no second web assembly to bind.
//!   * React's host-adapter binding → `app_with_adapter` / `run_with_adapter`
//!     (native host injection) plus the wasm `start()` entry point.
//!   * React's all-throw `createWebBackendTransportStub` → the unwired
//!     `UnimplementedAdapter` every `AppState` starts on: ports answer the
//!     fixed `DESKTOP_NOT_IMPLEMENTED` error, never a silent empty answer.
//!
//! The assertions below pin those existing Rune facts so the barrel's
//! disappearance is documented without inventing a shell.

use datazen_ui::AppState;
use datazen_ui_api::{DesktopAdapter, DESKTOP_NOT_IMPLEMENTED};
use rune::App;

/// The web form needs no second assembly: one public `run()` serves native and
/// wasm, and a native host can inject its own adapter. These compile-time
/// bindings pin that public surface (the React barrel only re-exported names).
#[test]
fn one_cross_platform_assembly_entry_with_native_adapter_injection() {
    let _run: fn() = datazen_ui::run;
    let _app_with_adapter: fn(Box<dyn DesktopAdapter>) -> App<AppState> = datazen_ui::app_with_adapter;
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _run_with_adapter: fn(Box<dyn DesktopAdapter>) = datazen_ui::run_with_adapter;
    }
}

/// The Rune equivalent of the all-throw web backend transport: a state with no
/// wired backend answers a fixed error instead of a silent empty list, while
/// keeping the same shape the desktop/native assembly binds.
#[test]
fn unwired_platform_port_errors_instead_of_silent_empty_answer() {
    let mut state = AppState::default();
    state.sync_desktop_connections();
    assert!(
        state.desktop_connections.is_none(),
        "no wired backend -> no connection list from the adapter"
    );
    assert_eq!(
        state.desktop_error.as_deref(),
        Some(DESKTOP_NOT_IMPLEMENTED),
        "the port must surface the fixed 'not implemented' error, not an empty answer"
    );
}
