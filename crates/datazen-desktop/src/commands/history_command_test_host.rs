//! Test-only command host for the history domain: drives the real
//! `#[tauri::command]` handlers.
//!
//! B2 batch-4 knife (commands-layer-review §4): `purge_history_impl` is gone,
//! so the purge/validations tests enter through the REAL command entries.
//! Their `State<'_, AppState>` parameters can only come from a tauri
//! `Manager`, hence this bridge: a `tauri::test::mock_app()` that manages the
//! harness's `AppState` (same shape as the AI / query / data and the other
//! domain knives' hosts).
//!
//! The history commands take no window or app-handle parameters (they emit no
//! events), so no mock webview window is needed.

use crate::commands::AppState;
use crate::testing::app_state_test_support::{TestAppState, TestStateKeepalive};
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
