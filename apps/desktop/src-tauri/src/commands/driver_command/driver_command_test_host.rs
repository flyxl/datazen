//! Test-only command host for the driver_command domain: drives the real
//! `#[tauri::command]` handlers.
//!
//! B2 driver_command 刀（commands-layer-review §3.3）：命令层持有接线逻辑，
//! 测试进真实命令函数。它们的 `State<'_, AppState>` 只能来自 tauri
//! `Manager`， hence this bridge（与 `commands/query_command_test_host.rs`
//! 同形——driver_command 的两个 discovery 命令不取窗口 / AppHandle）。
//!
//! `execute_driver_command`（AppHandle 参数）与 `execute_driver_command_stream`
//! （tauri IPC `Channel` 参数）暂不提供真入口托管：前者需要 Wry 句柄
//! （dialog 网关是具体类型），后者需要可构造的 Channel 测试面——相关测试
//! 仍驱动域内保留的跨域共享入口（`*_impl`），见 tests.rs 的分类注释。

use crate::commands::AppState;
use crate::testing::app_state::{TestAppState, TestStateKeepalive};
use tauri::Manager;

/// A mock tauri app hosting an [`AppState`] for direct command calls.
pub(super) struct CommandApp {
    app: tauri::App<tauri::test::MockRuntime>,
}

/// A [`TestAppState`] handed to a [`CommandApp`], with the parts that must
/// outlive the test (tempdir, store, mock driver, registry) kept alive.
///
/// The fields exist for their drop/lifetime semantics, but tests also keep
/// driving the mock through `keep.mock` after the state has been handed over.
pub(super) struct HostedCommandApp {
    pub(super) app: CommandApp,
    /// 持有必须比被托管状态活得久的部分（tempdir/store/mock/registry）。
    /// 本域的 discovery 测试不直读它——存在本身就是为了生命周期。
    #[allow(dead_code)]
    pub(super) keep: TestStateKeepalive,
}

impl std::ops::Deref for HostedCommandApp {
    type Target = CommandApp;

    fn deref(&self) -> &CommandApp {
        &self.app
    }
}

/// Host a harness's state behind a mock command app.
pub(super) fn host_command_app(test: TestAppState) -> HostedCommandApp {
    let (state, keep) = test.into_state();
    HostedCommandApp {
        app: CommandApp::new(state),
        keep,
    }
}

impl CommandApp {
    /// Manage `state` behind a fresh mock app.
    pub(super) fn new(state: AppState) -> Self {
        let app = tauri::test::mock_app();
        app.manage(state);
        Self { app }
    }

    /// The managed state, as a tauri `State` handle for command parameters.
    pub(super) fn state(&self) -> tauri::State<'_, AppState> {
        self.app.state::<AppState>()
    }
}
