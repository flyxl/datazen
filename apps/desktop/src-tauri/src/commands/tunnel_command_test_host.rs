//! Test-only command host for the tunnel domain: drives the real
//! `#[tauri::command]` handlers.
//!
//! B2 tunnel knife (commands-layer-review §3.3): the tunnel command layer
//! holds its wiring logic directly in the command handlers — the `_impl`
//! forwarding layer is gone — so tests enter through the command functions
//! themselves. Their `State<'_, AppState>` parameters can only come from a
//! tauri `Manager`, hence this bridge: a `tauri::test::mock_app()` that
//! manages the harness's `AppState` (same shape as
//! `commands/ai/command_test_host.rs`, shared with the query and
//! dashboard/config knives).
//!
//! The tunnel commands take no window or app-handle parameters (they emit no
//! events), so unlike the AI host this one needs no mock webview window.

use crate::commands::AppState;
use crate::testing::app_state::{TestAppState, TestStateKeepalive};
use tauri::Manager;

/// A mock tauri app hosting an [`AppState`] for direct command calls.
pub(crate) struct CommandApp {
    app: tauri::App<tauri::test::MockRuntime>,
}

/// A [`TestAppState`] handed to a [`CommandApp`], with the parts that must
/// outlive the test (tempdir, store, mock driver, registry) kept alive.
pub(crate) struct HostedCommandApp {
    pub(crate) app: CommandApp,
    pub(crate) keep: TestStateKeepalive,
}

impl std::ops::Deref for HostedCommandApp {
    type Target = CommandApp;

    fn deref(&self) -> &CommandApp {
        &self.app
    }
}

/// Host a harness's state behind a mock command app.
pub(crate) fn host_command_app(test: TestAppState) -> HostedCommandApp {
    let (state, keep) = test.into_state();
    HostedCommandApp {
        app: CommandApp::new(state),
        keep,
    }
}

impl CommandApp {
    /// Manage `state` behind a fresh mock app.
    pub(crate) fn new(state: AppState) -> Self {
        let app = tauri::test::mock_app();
        app.manage(state);
        Self { app }
    }

    /// The managed state, as a tauri `State` handle for command parameters.
    pub(crate) fn state(&self) -> tauri::State<'_, AppState> {
        self.app.state::<AppState>()
    }
}
