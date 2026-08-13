use super::error::{CmdExt, CommandError};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;
use tokio::io::AsyncWriteExt;

struct SaveSession {
    path: PathBuf,
    file: tokio::fs::File,
}

static SAVE_SESSIONS: LazyLock<tokio::sync::Mutex<HashMap<String, SaveSession>>> =
    LazyLock::new(|| tokio::sync::Mutex::new(HashMap::new()));

const ALLOWED_EXTENSIONS: &[&str] = &[
    "csv", "tsv", "json", "sql", "md", "txt", "xml", "yaml", "yml", "png", "svg", "zip", "gz",
    "dump", "xlsx",
];

fn deny_path_ipc() -> CommandError {
    CommandError::Validation("Direct path file IPC is disabled; use *_with_dialog commands".into())
}

fn validate_extension(path: &Path, allowed: &[&str]) -> Result<(), CommandError> {
    let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
        return Err(CommandError::Validation(
            "File must have an extension".into(),
        ));
    };
    let ext = ext.to_lowercase();
    if !allowed.iter().any(|a| a.eq_ignore_ascii_case(&ext)) {
        return Err(CommandError::Validation(format!(
            "File extension '.{ext}' not allowed"
        )));
    }
    Ok(())
}

fn validate_file_path(path: &Path) -> Result<(), CommandError> {
    if path.to_string_lossy().contains("..") {
        return Err(CommandError::Validation(
            "Path traversal not allowed".into(),
        ));
    }
    validate_extension(path, ALLOWED_EXTENSIONS)
}

fn dialog_path_to_buf(path: tauri_plugin_dialog::FilePath) -> Result<PathBuf, CommandError> {
    path.into_path()
        .map_err(|e| CommandError::Validation(format!("Invalid dialog path: {e}")))
}

fn ext_refs(extensions: &[String]) -> Result<Vec<&str>, CommandError> {
    if extensions.is_empty() {
        return Err(CommandError::Validation(
            "At least one file extension is required".into(),
        ));
    }
    for ext in extensions {
        let clean = ext.trim_start_matches('.').to_lowercase();
        if !ALLOWED_EXTENSIONS.contains(&clean.as_str()) {
            return Err(CommandError::Validation(format!(
                "File extension '.{clean}' not allowed"
            )));
        }
    }
    Ok(extensions
        .iter()
        .map(|e| {
            let s = e.as_str();
            if let Some(stripped) = s.strip_prefix('.') {
                stripped
            } else {
                s
            }
        })
        .collect())
}

/// Save UTF-8 text via a native save dialog. Path never returns to the webview.
/// Returns `true` if saved, `false` if the user cancelled.
#[tauri::command]
pub async fn save_text_with_dialog(
    app: AppHandle,
    contents: String,
    default_file_name: String,
    filter_name: String,
    extensions: Vec<String>,
) -> Result<bool, CommandError> {
    let ext_list = ext_refs(&extensions)?;
    let picked = app
        .dialog()
        .file()
        .add_filter(&filter_name, &ext_list)
        .set_file_name(&default_file_name)
        .blocking_save_file();
    let Some(fp) = picked else {
        return Ok(false);
    };
    let path = dialog_path_to_buf(fp)?;
    validate_extension(&path, &ext_list)?;
    tokio::fs::write(&path, contents.as_bytes())
        .await
        .cmd_err("save_text_with_dialog")?;
    Ok(true)
}

/// Save base64-decoded bytes via a native save dialog.
#[tauri::command]
pub async fn save_base64_with_dialog(
    app: AppHandle,
    data_base64: String,
    default_file_name: String,
    filter_name: String,
    extensions: Vec<String>,
) -> Result<bool, CommandError> {
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
    let ext_list = ext_refs(&extensions)?;
    let bytes = BASE64
        .decode(data_base64.trim())
        .map_err(|e| CommandError::Validation(format!("Invalid base64: {e}")))?;
    let picked = app
        .dialog()
        .file()
        .add_filter(&filter_name, &ext_list)
        .set_file_name(&default_file_name)
        .blocking_save_file();
    let Some(fp) = picked else {
        return Ok(false);
    };
    let path = dialog_path_to_buf(fp)?;
    validate_extension(&path, &ext_list)?;
    tokio::fs::write(&path, bytes)
        .await
        .cmd_err("save_base64_with_dialog")?;
    Ok(true)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenedTextFile {
    /// Basename only — safe to show in UI; do not use for subsequent IO.
    pub file_name: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenedBinaryFile {
    /// Basename only — safe to show in UI; do not use for subsequent IO.
    pub file_name: String,
    pub data_base64: String,
}

/// Open a binary file via native dialog and return base64 contents (no path to JS).
#[tauri::command]
pub async fn open_base64_with_dialog(
    app: AppHandle,
    filter_name: String,
    extensions: Vec<String>,
) -> Result<Option<OpenedBinaryFile>, CommandError> {
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
    let ext_list = ext_refs(&extensions)?;
    let picked = app
        .dialog()
        .file()
        .add_filter(&filter_name, &ext_list)
        .blocking_pick_file();
    let Some(fp) = picked else {
        return Ok(None);
    };
    let path = dialog_path_to_buf(fp)?;
    validate_extension(&path, &ext_list)?;
    let bytes = tokio::fs::read(&path)
        .await
        .cmd_err("open_base64_with_dialog")?;
    let file_name = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".into());
    let data_base64 = BASE64.encode(bytes);
    Ok(Some(OpenedBinaryFile {
        file_name,
        data_base64,
    }))
}

/// Open a text file via native dialog and return its contents (no path to JS).
#[tauri::command]
pub async fn open_text_with_dialog(
    app: AppHandle,
    filter_name: String,
    extensions: Vec<String>,
) -> Result<Option<OpenedTextFile>, CommandError> {
    let ext_list = ext_refs(&extensions)?;
    let picked = app
        .dialog()
        .file()
        .add_filter(&filter_name, &ext_list)
        .blocking_pick_file();
    let Some(fp) = picked else {
        return Ok(None);
    };
    let path = dialog_path_to_buf(fp)?;
    validate_extension(&path, &ext_list)?;
    let content = tokio::fs::read_to_string(&path)
        .await
        .cmd_err("open_text_with_dialog")?;
    let file_name = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".into());
    Ok(Some(OpenedTextFile { file_name, content }))
}

async fn insert_save_session(path: PathBuf) -> Result<String, CommandError> {
    let file = tokio::fs::File::create(&path)
        .await
        .cmd_err("begin_save_with_dialog")?;
    let token = uuid::Uuid::new_v4().to_string();
    let mut sessions = SAVE_SESSIONS.lock().await;
    if sessions.len() >= 8 {
        return Err(CommandError::Validation(
            "Too many concurrent save sessions".into(),
        ));
    }
    sessions.insert(token.clone(), SaveSession { path, file });
    Ok(token)
}

/// Open a save dialog and keep an opaque write handle. Path never returns to JS.
#[tauri::command]
pub async fn begin_save_with_dialog(
    app: AppHandle,
    default_file_name: String,
    filter_name: String,
    extensions: Vec<String>,
) -> Result<Option<String>, CommandError> {
    let ext_list = ext_refs(&extensions)?;
    let picked = app
        .dialog()
        .file()
        .add_filter(&filter_name, &ext_list)
        .set_file_name(&default_file_name)
        .blocking_save_file();
    let Some(fp) = picked else {
        return Ok(None);
    };
    let path = dialog_path_to_buf(fp)?;
    validate_extension(&path, &ext_list)?;
    let token = insert_save_session(path).await?;
    Ok(Some(token))
}

#[tauri::command]
pub async fn append_save_text(token: String, chunk: String) -> Result<(), CommandError> {
    let mut sessions = SAVE_SESSIONS.lock().await;
    let session = sessions
        .get_mut(&token)
        .ok_or_else(|| CommandError::NotFound("Save session not found".into()))?;
    session
        .file
        .write_all(chunk.as_bytes())
        .await
        .cmd_err("append_save_text")?;
    Ok(())
}

#[tauri::command]
pub async fn finish_save(token: String) -> Result<(), CommandError> {
    let mut sessions = SAVE_SESSIONS.lock().await;
    let mut session = sessions
        .remove(&token)
        .ok_or_else(|| CommandError::NotFound("Save session not found".into()))?;
    session.file.flush().await.cmd_err("finish_save")?;
    Ok(())
}

#[tauri::command]
pub async fn abort_save(token: String) -> Result<(), CommandError> {
    let mut sessions = SAVE_SESSIONS.lock().await;
    let session = sessions
        .remove(&token)
        .ok_or_else(|| CommandError::NotFound("Save session not found".into()))?;
    drop(session.file);
    let _ = tokio::fs::remove_file(&session.path).await;
    Ok(())
}

/// Legacy path-based write — only available in webdriver/E2E builds.
#[tauri::command]
pub async fn write_file(path: String, contents: String) -> Result<(), CommandError> {
    write_file_impl(path, contents).await
}

#[tauri::command]
pub async fn write_file_base64(path: String, data_base64: String) -> Result<(), CommandError> {
    if !cfg!(feature = "webdriver") {
        return Err(deny_path_ipc());
    }
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
    let p = PathBuf::from(&path);
    validate_file_path(&p)?;
    let bytes = BASE64
        .decode(data_base64.trim())
        .map_err(|e| CommandError::Validation(format!("Invalid base64: {e}")))?;
    tokio::fs::write(&p, bytes)
        .await
        .cmd_err("write_file_base64")
}

pub(crate) async fn read_file_impl(path: String) -> Result<String, CommandError> {
    if !cfg!(feature = "webdriver") {
        return Err(deny_path_ipc());
    }
    let p = PathBuf::from(&path);
    validate_file_path(&p)?;
    tokio::fs::read_to_string(&p).await.cmd_err("read_file")
}

pub(crate) async fn write_file_impl(path: String, contents: String) -> Result<(), CommandError> {
    if !cfg!(feature = "webdriver") {
        return Err(deny_path_ipc());
    }
    let p = PathBuf::from(&path);
    validate_file_path(&p)?;
    tokio::fs::write(&p, contents.as_bytes())
        .await
        .cmd_err("write_file")
}

#[tauri::command]
pub async fn read_file(path: String) -> Result<String, CommandError> {
    read_file_impl(path).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ext_refs_accepts_dot_prefix_and_normalizes() {
        let extensions = [".json".into(), "SQL".into()];
        let refs = ext_refs(&extensions).unwrap();
        assert_eq!(refs, vec!["json", "SQL"]);
    }

    #[test]
    fn ext_refs_rejects_empty_list() {
        assert!(ext_refs(&[]).is_err());
    }

    #[test]
    fn validate_extension_accepts_case_insensitive() {
        assert!(validate_extension(Path::new("/tmp/file.JSON"), &["json"]).is_ok());
    }

    #[test]
    fn validate_file_path_accepts_allowed_extensions() {
        let valid = [
            "/tmp/test.csv",
            "/home/user/data.json",
            "/var/data/query.sql",
            "notes.md",
            "readme.txt",
            "config.xml",
            "settings.yaml",
            "settings.yml",
            "diagram.png",
            "diagram.svg",
            "backup.zip",
            "data.xlsx",
        ];
        for path in valid {
            assert!(
                validate_file_path(Path::new(path)).is_ok(),
                "expected valid path: {path}"
            );
        }
    }

    #[test]
    fn validate_file_path_rejects_path_traversal() {
        let err = validate_file_path(Path::new("../../../etc/passwd")).unwrap_err();
        assert_eq!(err.to_string(), "Path traversal not allowed");
    }

    #[test]
    fn validate_file_path_rejects_disallowed_extensions() {
        for path in ["malware.exe", "script.sh", "source.rs"] {
            let err = validate_file_path(Path::new(path)).unwrap_err();
            let msg = err.to_string();
            assert!(
                msg.starts_with("File extension '.") && msg.ends_with("' not allowed"),
                "unexpected error for {path}: {msg}"
            );
        }
    }

    #[test]
    fn validate_file_path_rejects_missing_extension() {
        let err = validate_file_path(Path::new("/tmp/noext")).unwrap_err();
        assert_eq!(err.to_string(), "File must have an extension");
    }

    #[test]
    fn ext_refs_rejects_unknown_extension() {
        let err = ext_refs(&["exe".into()]).unwrap_err();
        assert!(err.to_string().contains("not allowed"));
    }

    #[test]
    fn path_ipc_write_file_gated_without_webdriver() {
        if cfg!(feature = "webdriver") {
            return;
        }
        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(write_file("/tmp/e2e-gate-test.txt".into(), "x".into()));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("disabled"));
    }

    #[test]
    fn path_ipc_read_file_gated_without_webdriver() {
        if cfg!(feature = "webdriver") {
            return;
        }
        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(read_file("/tmp/e2e-gate-test.txt".into()));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("disabled"));
    }

    #[test]
    fn path_ipc_write_file_base64_gated_without_webdriver() {
        if cfg!(feature = "webdriver") {
            return;
        }
        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(write_file_base64(
                "/tmp/e2e-gate-test.png".into(),
                "AAAA".into(),
            ));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("disabled"));
    }

    #[tokio::test]
    async fn save_session_appends_then_finish() {
        let dir = std::env::temp_dir().join(format!("dz-save-{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(&dir).await.unwrap();
        let path = dir.join("out.csv");
        let token = insert_save_session(path.clone()).await.unwrap();
        append_save_text(token.clone(), "id,name\n".into())
            .await
            .unwrap();
        append_save_text(token.clone(), "1,a\n".into())
            .await
            .unwrap();
        finish_save(token).await.unwrap();
        let body = tokio::fs::read_to_string(&path).await.unwrap();
        assert_eq!(body, "id,name\n1,a\n");
        let _ = tokio::fs::remove_dir_all(&dir).await;
    }

    #[tokio::test]
    async fn save_session_abort_deletes_file() {
        let dir = std::env::temp_dir().join(format!("dz-abort-{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(&dir).await.unwrap();
        let path = dir.join("out.json");
        let token = insert_save_session(path.clone()).await.unwrap();
        append_save_text(token.clone(), "[".into()).await.unwrap();
        abort_save(token).await.unwrap();
        assert!(!path.exists());
        let _ = tokio::fs::remove_dir_all(&dir).await;
    }

    #[tokio::test]
    async fn append_unknown_token_errors() {
        let err = append_save_text("missing".into(), "x".into())
            .await
            .unwrap_err();
        assert!(err.to_string().contains("not found") || err.to_string().contains("NotFound"));
    }
}
