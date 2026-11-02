//! WR-C3 settings 域的测试宿主：mock_app 托管真实 `AppState`。
//!
//! 沿 `dashboard_command_test_host` 模式（一域一宿）：settings 命令
//! （`get_settings` / `save_settings`）的 `State<'_, AppState>` 参数只能
//! 来自 tauri `Manager`，契约测试经 `tauri::test::mock_app()` 托管
//! `TestAppState` 组装好的状态，直接驱动真实 `#[tauri::command]` 入口。
//! 宿主装配里 `PlatformAdapter::inject` 挂的设置用例（
//! `DesktopSettingsRepository` over 同一个 `DesktopRepository`）即生产
//! 路径同款端口。

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
    fn new(state: AppState) -> Self {
        let app = tauri::test::mock_app();
        app.manage(state);
        Self { app }
    }

    /// The managed state, as a tauri `State` handle for command parameters.
    pub(crate) fn state(&self) -> tauri::State<'_, AppState> {
        self.app.state::<AppState>()
    }
}
