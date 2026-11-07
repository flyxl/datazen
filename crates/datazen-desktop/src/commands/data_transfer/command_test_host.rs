//! Test-only command host for the data_transfer domain: drives the real
//! `#[tauri::command]` handlers.
//!
//! B2 data_transfer 域刀（commands-layer-review §3.5）：`cancel_data_transfer_impl`
//! 转发体拆除后，钉 never-`Ok(true)` 面的测试经真实命令入口驱动——命令的
//! `State<'_, AppState>` 参数只能来自 tauri `Manager`，此桥与
//! `commands/sync/command_test_host.rs` 同形（mock_app 托管 harness 的
//! `AppState`；本域被驱动的命令不接窗口/AppHandle，无需 mock webview）。

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
