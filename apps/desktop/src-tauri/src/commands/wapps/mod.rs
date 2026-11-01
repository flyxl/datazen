//! Runtime UI-extension IPC: list / install / remove / enable, manifest lookup,
//! per-extension KV storage, and sandbox-constrained file reads.
//!
//! B2 wapps 域刀（commands-layer-review §3.6）：`execute_wapp_driver_command_impl`
//! 与 `inspect_wapp_package_impl` 两个转发体已删——接线直接住在
//! `#[tauri::command]` 处理函数里；`WappSummary` 投影与 `WappPackageKind`
//! 词汇解析已下沉 `datazen::workspace_apps::{summary, package_kind}`（线契约
//! 逐字保留）。registry/storage 的九个 `*_impl` 因**跨域调用方**保留（见
//! 各自的调用方清单注释）：`src/wapps/integration_tests.rs`（协议桥的
//! 集成测试，只读钉子）直调它们驱动宿主命令层。
//!
//! IPC 契约逐字节不变：命令名、参数（serde camelCase）、`wapps:changed`
//! 事件与错误文案。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::LazyLock;

use serde_json::Value;
use tauri::{AppHandle, Emitter, State};

use super::error::{resolve_override_path, CmdExt, CommandError, OVERRIDE_DISABLED_MSG};
use super::AppState;
use datazen::workspace_apps::install::{install_from_dir, install_from_zip};
use datazen::workspace_apps::manager::LoadedWapp;
use datazen::workspace_apps::storage::{storage_get, storage_remove, storage_set};
use datazen::workspace_apps::WappManifest;

// B2 域刀下沉物的宿主侧引用：投影与词汇解析的规则本体在 modules，
// 这里只保留类型路径（`commands::wapps::WappSummary` 等旧路径不变）。
pub use datazen::workspace_apps::summary::{WappPageSummary, WappSummary, WappThemeSummary};
pub(crate) use datazen::workspace_apps::package_kind::{package_label, WappPackageKind};

pub const WAPPS_CHANGED_EVENT: &str = "wapps:changed";

const MAX_PICK_SESSIONS: usize = 8;

struct WappPickSession {
    path: PathBuf,
}

static WAPP_PICK_SESSIONS: LazyLock<tokio::sync::Mutex<HashMap<String, WappPickSession>>> =
    LazyLock::new(|| tokio::sync::Mutex::new(HashMap::new()));

async fn insert_pick_session(path: PathBuf) -> Result<String, CommandError> {
    let token = uuid::Uuid::new_v4().to_string();
    let mut sessions = WAPP_PICK_SESSIONS.lock().await;
    if sessions.len() >= MAX_PICK_SESSIONS {
        return Err(CommandError::Validation(
            "Too many pending wapp picks".into(),
        ));
    }
    sessions.insert(token.clone(), WappPickSession { path });
    Ok(token)
}

async fn take_pick_session(token: &str) -> Result<PathBuf, CommandError> {
    let mut sessions = WAPP_PICK_SESSIONS.lock().await;
    sessions
        .remove(token)
        .map(|session| session.path)
        .ok_or_else(|| CommandError::NotFound("Wapp pick session not found or expired".into()))
}

pub(crate) async fn pick_wapp_package_with_dialog(
    app: &AppHandle,
    kind: WappPackageKind,
) -> Result<Option<PathBuf>, CommandError> {
    match kind {
        WappPackageKind::Zip => {
            super::dialog::open_file(app, vec![("Wapp package".into(), vec!["zip".into()])]).await
        }
        WappPackageKind::Folder => super::dialog::pick_folder(app).await,
    }
}

async fn resolve_wapp_package_path(
    app: &AppHandle,
    kind: WappPackageKind,
    override_path: Option<String>,
) -> Result<Option<PathBuf>, CommandError> {
    match resolve_override_path(override_path, OVERRIDE_DISABLED_MSG)? {
        Some(path) => Ok(Some(path)),
        None => pick_wapp_package_with_dialog(app, kind).await,
    }
}

fn ensure_wapp_exists(state: &AppState, id: &str) -> Result<LoadedWapp, CommandError> {
    state
        .wapps
        .get(id)
        .ok_or_else(|| CommandError::NotFound(format!("wapp not found: {id}")))
}

mod driver;
mod sessions;
pub use sessions::*;
mod registry;
mod storage;

pub use driver::*;
pub use registry::*;
pub use storage::*;

#[cfg(test)]
mod command_test_host;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
