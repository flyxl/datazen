//! Test-only command host for the wapps domain: drives the real
//! `#[tauri::command]` handlers.
//!
//! B2 wapps 域刀（commands-layer-review §3.6）：`execute_wapp_driver_command_impl`
//! 转发层已删，测试进真实命令函数。它的 `State<'_, AppState>` 只能来自
//! tauri `Manager`，故有此桥（与 `commands/schema/command_test_host.rs`
//! 同形——本域的 driver 命令不取窗口 / AppHandle）。
//!
//! 带 `AppHandle` / 对话框的 registry 命令（`install_wapp` /
//! `remove_wapp` / `set_wapp_enabled` / `inspect_wapp_package_with_dialog`）
//! 暂不提供真入口托管（Wry 句柄 + webdriver 路径门），相关测试仍驱动域内
//! 保留的跨域共享入口（`*_impl`），见 tests.rs 的分类注释。

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
    /// 持有必须比被托管状态活得久的部分（tempdir/store/mock/registry）。
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
