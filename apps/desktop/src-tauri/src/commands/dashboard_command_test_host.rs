//! Test-only command host: drives the real `#[tauri::command]` handlers.
//!
//! The dashboard and config command layers hold their wiring logic directly
//! in the command handlers (B2 knife: no `_impl` forwarding layer), so tests
//! must enter through the command functions themselves. Their
//! `State<'_, AppState>` parameters can only come from a tauri `Manager`,
//! hence this bridge: a `tauri::test::mock_app()` that manages the harness's
//! `AppState` — the same shape `commands/ai/command_test_host.rs` introduced
//! for the WR-A2 AI knife. One host per knife, shared by the two domains this
//! knife owns (`commands/dashboard.rs` and `commands/config.rs` tests).

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
