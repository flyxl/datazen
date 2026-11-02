//! Test-only command host for the schema_diff domain: drives the real
//! `#[tauri::command]` handlers.
//!
//! B2 schema_diff knife (commands-layer-review §3.5): the plan/deploy `_impl`
//! forwarding bodies are gone (their orchestration lives in `job.rs` behind
//! the JobRuntime backend), so plan/deploy tests enter through the command
//! functions themselves and poll `get_schema_diff_job_details` for the
//! durable outcome — the same surface the frontend drives. Their
//! `State<'_, AppState>` parameters can only come from a tauri `Manager`,
//! hence this bridge: a `tauri::test::mock_app()` that manages the harness's
//! `AppState` (same shape as `commands/query_command_test_host.rs`).
//!
//! The schema_diff commands take no window or app-handle parameters (stream
//! traffic rides the Job surface), so unlike the AI host this one needs no
//! mock webview window.

use crate::commands::AppState;
use crate::testing::app_state_test_support::{TestAppState, TestStateKeepalive};
use tauri::Manager;

/// A mock tauri app hosting an [`AppState`] for direct command calls.
pub(crate) struct CommandApp {
    app: tauri::App<tauri::test::MockRuntime>,
}

/// A [`TestAppState`] handed to a [`CommandApp`], with the parts that must
/// outlive the test (tempdir, store, mock driver, registry) kept alive.
///
/// The fields exist for their drop/lifetime semantics, but tests also keep
/// driving the mock through `keep.mock` after the state has been handed over.
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
