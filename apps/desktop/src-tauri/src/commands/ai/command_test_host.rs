//! Test-only command host: drives the real `#[tauri::command]` handlers.
//!
//! The AI command layer holds its wiring logic directly in the command
//! handlers (no `_impl` forwarding layer), so tests must enter through the
//! command functions themselves. Their `State<'_, AppState>` /
//! `WebviewWindow` / `AppHandle` parameters can only come from a tauri
//! `Manager`, hence this bridge: a `tauri::test::mock_app()` that manages the
//! harness's `AppState` and owns a mock webview window.
//!
//! Streaming assertions observe the real transport: the commands emit
//! `ai:stream-chunk` / `ai:stream-error` events, and tauri's in-process event
//! system delivers them to listeners registered on the mock window — the same
//! code path the desktop webview uses in production.

use crate::commands::AppState;
use crate::testing::app_state::{TestAppState, TestStateKeepalive};
use std::sync::{Arc, Mutex};
use tauri::{Listener, Manager};

/// A mock tauri app hosting an [`AppState`] for direct command calls.
pub(crate) struct CommandApp {
    app: tauri::App<tauri::test::MockRuntime>,
    window: tauri::WebviewWindow<tauri::test::MockRuntime>,
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
    /// Manage `state` behind a fresh mock app and create its main window.
    pub(crate) fn new(state: AppState) -> Self {
        let app = tauri::test::mock_app();
        app.manage(state);
        let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .expect("mock webview window");
        Self { app, window }
    }

    /// The managed state, as a tauri `State` handle for command parameters.
    pub(crate) fn state(&self) -> tauri::State<'_, AppState> {
        self.app.state::<AppState>()
    }

    /// The mock app handle (for commands that emit config-change events).
    pub(crate) fn handle(&self) -> tauri::AppHandle<tauri::test::MockRuntime> {
        self.app.handle().clone()
    }

    /// The mock main window (for streaming commands).
    pub(crate) fn window(&self) -> tauri::WebviewWindow<tauri::test::MockRuntime> {
        self.window.clone()
    }

    /// Collect the stream traffic the command emits as `(requestId, done)`
    /// pairs — the test-side replacement for the former injected
    /// `StreamCallback`. Chunk events record their `done` flag; error events
    /// record `false`.
    pub(crate) fn stream_log(&self) -> Arc<Mutex<Vec<(String, bool)>>> {
        let log: Arc<Mutex<Vec<(String, bool)>>> = Arc::new(Mutex::new(Vec::new()));
        let chunk_log = log.clone();
        self.window.listen("ai:stream-chunk", move |event| {
            if let Ok(payload) = serde_json::from_str::<serde_json::Value>(event.payload()) {
                let request_id = payload["requestId"].as_str().unwrap_or_default().to_string();
                let done = payload["done"].as_bool().unwrap_or(false);
                chunk_log.lock().unwrap().push((request_id, done));
            }
        });
        let error_log = log.clone();
        self.window.listen("ai:stream-error", move |event| {
            if let Ok(payload) = serde_json::from_str::<serde_json::Value>(event.payload()) {
                let request_id = payload["requestId"].as_str().unwrap_or_default().to_string();
                error_log.lock().unwrap().push((request_id, false));
            }
        });
        log
    }
}
