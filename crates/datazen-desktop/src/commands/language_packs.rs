//! Language pack IPC: list / read messages / inspect (validate-only) /
//! install / remove.
//!
//! 宿主薄壳（设计档：`docs/development/language-pack-install-design.md`）：
//! 规则本体在 `datazen::language_packs`（校验/安装/扫描/卸载全部下沉），
//! 这里只保留桌面形态件——原生文件对话框、pick 会话（选包凭证）、
//! webdriver-only 的 override 逃生门、`language-packs:changed` 事件与
//! `spawn_blocking` 包裹的文件 IO（形状与 `commands/wapps` 一致）。
//!
//! IPC 契约：命令名、参数（serde camelCase）、事件与错误文案。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::LazyLock;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use super::error::{resolve_override_path, CommandError, OVERRIDE_DISABLED_MSG};
use super::AppState;
use datazen::language_packs::{
    inspect_language_pack_file, install_from_zip, LanguagePackDomainCount, LanguagePackManifest,
    LanguagePackMessages, LanguagePackSummary,
};

pub const LANGUAGE_PACKS_CHANGED_EVENT: &str = "language-packs:changed";

const MAX_PICK_SESSIONS: usize = 8;

struct LanguagePackPickSession {
    path: PathBuf,
}

static LANGUAGE_PACK_PICK_SESSIONS: LazyLock<
    tokio::sync::Mutex<HashMap<String, LanguagePackPickSession>>,
> = LazyLock::new(|| tokio::sync::Mutex::new(HashMap::new()));

async fn insert_pick_session(path: PathBuf) -> Result<String, CommandError> {
    let token = uuid::Uuid::new_v4().to_string();
    let mut sessions = LANGUAGE_PACK_PICK_SESSIONS.lock().await;
    if sessions.len() >= MAX_PICK_SESSIONS {
        return Err(CommandError::Validation(
            "Too many pending language pack picks".into(),
        ));
    }
    sessions.insert(token.clone(), LanguagePackPickSession { path });
    Ok(token)
}

async fn take_pick_session(token: &str) -> Result<PathBuf, CommandError> {
    let mut sessions = LANGUAGE_PACK_PICK_SESSIONS.lock().await;
    sessions
        .remove(token)
        .map(|session| session.path)
        .ok_or_else(|| {
            CommandError::NotFound("Language pack pick session not found or expired".into())
        })
}

/// Shared error mapping: not-found 文案 → NotFound 桶，其余校验失败 →
/// Validation 桶（错误本体逐字来自共享层）。
fn map_pack_error(error: &str) -> CommandError {
    if error.starts_with("language pack not found")
        || error.starts_with("language pack file not found")
    {
        CommandError::NotFound(error.to_string())
    } else {
        CommandError::Validation(error.to_string())
    }
}

/// 用户所选文件的显示名（`package` 兜底）。
fn package_label(path: &std::path::Path) -> String {
    path.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "package".into())
}

async fn resolve_language_pack_path(
    app: &AppHandle,
    override_path: Option<String>,
) -> Result<Option<PathBuf>, CommandError> {
    match resolve_override_path(override_path, OVERRIDE_DISABLED_MSG)? {
        Some(path) => Ok(Some(path)),
        None => {
            super::dialog::open_file(app, vec![("Language pack".into(), vec!["zip".into()])]).await
        }
    }
}

/// 跨域共享入口：生产调用方 `list_language_packs` 命令；测试驱动同款。
pub(crate) fn list_language_packs_impl(state: &AppState) -> Vec<LanguagePackSummary> {
    state.language_packs.list()
}

/// 跨域共享入口：生产调用方 `read_language_pack_messages` 命令；测试同款。
pub(crate) fn read_language_pack_messages_impl(
    state: &AppState,
    code: &str,
) -> Result<LanguagePackMessages, CommandError> {
    state
        .language_packs
        .read_messages(code)
        .map_err(|e| map_pack_error(&e))
}

/// 跨域共享入口（pick token/override path 解析是宿主形态，留在命令里）：
/// 生产调用方 `install_language_pack` 命令；测试同款。
pub(crate) async fn install_language_pack_from_path_impl(
    state: &AppState,
    path: String,
) -> Result<LanguagePackSummary, CommandError> {
    let source = PathBuf::from(&path);
    if !source.exists() {
        return Err(CommandError::NotFound(format!(
            "language pack file not found: {}",
            source.display()
        )));
    }

    let packs_dir = state.language_packs.packs_dir().to_path_buf();
    let inspection = tokio::task::spawn_blocking(move || install_from_zip(&source, &packs_dir))
        .await
        .map_err(|e| CommandError::Internal(format!("install_language_pack task: {e}")))?
        .map_err(|e| map_pack_error(&e))?;

    let message_count: usize = inspection.domains.iter().map(|d| d.message_count).sum();
    state
        .language_packs
        .register(inspection.manifest.clone(), message_count);
    tracing::info!(
        code = %inspection.manifest.code,
        version = %inspection.manifest.version,
        "install_language_pack OK"
    );

    Ok(LanguagePackSummary {
        code: inspection.manifest.code.clone(),
        label: inspection.manifest.label.clone(),
        version: inspection.manifest.version.clone(),
        author: inspection.manifest.author.clone(),
        description: inspection.manifest.description.clone(),
        message_count,
        file_name: inspection.manifest.code.clone(),
    })
}

/// 跨域共享入口：生产调用方 `remove_language_pack` 命令；测试同款。
pub(crate) async fn remove_language_pack_impl(
    state: &AppState,
    code: String,
) -> Result<(), CommandError> {
    state
        .language_packs
        .remove(&code)
        .map_err(|e| map_pack_error(&e))?;
    tracing::info!(%code, "remove_language_pack OK");
    Ok(())
}

#[tauri::command]
pub async fn list_language_packs(
    state: State<'_, AppState>,
) -> Result<Vec<LanguagePackSummary>, CommandError> {
    Ok(list_language_packs_impl(&state))
}

#[tauri::command]
pub async fn read_language_pack_messages(
    state: State<'_, AppState>,
    code: String,
) -> Result<LanguagePackMessages, CommandError> {
    read_language_pack_messages_impl(&state, &code)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguagePackPreview {
    pub pick_token: String,
    pub package_label: String,
    pub manifest: LanguagePackManifest,
    pub message_count: usize,
    pub domains: Vec<LanguagePackDomainCount>,
}

/// 接线（形状沿 `inspect_wapp_package_with_dialog`）：内核的完整包校验
/// （`datazen::language_packs::inspect_language_pack_file`——不落盘、物化进
/// 临时目录后跑同一套规则）在阻塞线程上执行；错误映射 NotFound（路径
/// 缺失）/ Validation（校验失败）；用户取消对话框返回 `Ok(None)`。
#[tauri::command]
pub async fn inspect_language_pack_file_with_dialog(
    app: AppHandle,
    override_path: Option<String>,
) -> Result<Option<LanguagePackPreview>, CommandError> {
    let Some(source) = resolve_language_pack_path(&app, override_path).await? else {
        return Ok(None);
    };
    if !source.exists() {
        return Err(CommandError::NotFound(format!(
            "language pack file not found: {}",
            source.display()
        )));
    }

    let inspection = {
        // 阻塞线程只拿走自己需要的路径；宿主侧的 pick 会话与标签仍读源路径。
        let source_for_task = source.clone();
        tokio::task::spawn_blocking(move || inspect_language_pack_file(&source_for_task))
            .await
            .map_err(|e| CommandError::Internal(format!("inspect_language_pack task: {e}")))?
            .map_err(|e| map_pack_error(&e))?
    };
    let pick_token = insert_pick_session(source.clone()).await?;
    let message_count: usize = inspection.domains.iter().map(|d| d.message_count).sum();
    Ok(Some(LanguagePackPreview {
        pick_token,
        package_label: package_label(&source),
        manifest: inspection.manifest,
        message_count,
        domains: inspection.domains,
    }))
}

#[tauri::command]
pub async fn install_language_pack(
    app: AppHandle,
    state: State<'_, AppState>,
    pick_token: Option<String>,
    override_path: Option<String>,
) -> Result<LanguagePackSummary, CommandError> {
    let path = match resolve_override_path(override_path, OVERRIDE_DISABLED_MSG)? {
        Some(path) => path.to_string_lossy().into_owned(),
        None => {
            let Some(token) = pick_token else {
                return Err(CommandError::Validation(
                    "No language pack file selected".into(),
                ));
            };
            take_pick_session(&token)
                .await?
                .to_string_lossy()
                .into_owned()
        }
    };

    let summary = install_language_pack_from_path_impl(&state, path).await?;
    let _ = app.emit(LANGUAGE_PACKS_CHANGED_EVENT, ());
    Ok(summary)
}

#[tauri::command]
pub async fn remove_language_pack(
    app: AppHandle,
    state: State<'_, AppState>,
    code: String,
) -> Result<(), CommandError> {
    remove_language_pack_impl(&state, code).await?;
    let _ = app.emit(LANGUAGE_PACKS_CHANGED_EVENT, ());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::app_state_test_support::TestAppState;
    use std::fs::{self, File};
    use std::io::Write;
    use std::path::Path;

    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    fn write_pack_zip(zip_path: &Path, code: &str, label: &str) {
        let file = File::create(zip_path).unwrap();
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        zip.start_file("manifest.json", options).unwrap();
        zip.write_all(
            format!(r#"{{"packVersion":1,"code":"{code}","label":"{label}","version":"1.0.0"}}"#)
                .as_bytes(),
        )
        .unwrap();
        zip.start_file("settings.json", options).unwrap();
        zip.write_all(r#"{"settings.language": "言語"}"#.as_bytes())
            .unwrap();
        zip.finish().unwrap();
    }

    #[tokio::test]
    async fn install_list_read_remove_round_trip() {
        let test = TestAppState::new().await;
        let state = &test.state;

        // AppState 组装：语言包注册表指向 {appData}/language-packs。
        assert!(state.language_packs.packs_dir().ends_with("language-packs"));

        let zip_path = test._temp.path().join("ja.zip");
        write_pack_zip(&zip_path, "ja", "日本語");

        // 装前为空。
        assert!(list_language_packs_impl(state).is_empty());

        let summary =
            install_language_pack_from_path_impl(state, zip_path.to_string_lossy().into_owned())
                .await
                .unwrap();
        assert_eq!(summary.code, "ja");
        assert_eq!(summary.label, "日本語");
        assert_eq!(summary.message_count, 1);

        // 列表 + 读译文。
        let list = list_language_packs_impl(state);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].code, "ja");
        let pack = read_language_pack_messages_impl(state, "ja").unwrap();
        assert_eq!(pack.label, "日本語");
        assert_eq!(
            pack.messages.get("settings.language"),
            Some(&"言語".to_string())
        );

        // 未登记的 code → NotFound 桶。
        let err = read_language_pack_messages_impl(state, "de").unwrap_err();
        assert!(matches!(err, CommandError::NotFound(_)), "{err:?}");

        // 卸载后列表清空、读取失败。
        remove_language_pack_impl(state, "ja".into()).await.unwrap();
        assert!(list_language_packs_impl(state).is_empty());
        assert!(read_language_pack_messages_impl(state, "ja").is_err());
    }

    #[tokio::test]
    async fn install_missing_file_maps_to_not_found() {
        let test = TestAppState::new().await;
        let err = install_language_pack_from_path_impl(
            &test.state,
            test._temp
                .path()
                .join("nope.zip")
                .to_string_lossy()
                .into_owned(),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, CommandError::NotFound(_)), "{err:?}");
    }

    #[tokio::test]
    async fn install_invalid_pack_maps_to_validation_with_shared_text() {
        let test = TestAppState::new().await;
        let zip_path = test._temp.path().join("builtin.zip");
        // 内置冲突的 code → 共享层文案原样透传到 Validation 桶。
        write_pack_zip(&zip_path, "en", "English");
        let err = install_language_pack_from_path_impl(
            &test.state,
            zip_path.to_string_lossy().into_owned(),
        )
        .await
        .unwrap_err();
        match err {
            CommandError::Validation(text) => {
                assert!(text.contains("conflicts with built-in"), "{text}");
            }
            other => panic!("expected Validation, got {other:?}"),
        }
        // 失败安装不登记。
        assert!(list_language_packs_impl(&test.state).is_empty());
    }

    #[test]
    fn package_label_uses_file_name() {
        assert_eq!(
            package_label(Path::new("/tmp/datazen-locale-ja.zip")),
            "datazen-locale-ja.zip"
        );
        assert_eq!(package_label(Path::new("/tmp/")), "package");
    }
}
