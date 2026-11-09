use super::error::{CmdExt, CommandError};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use tauri::AppHandle;
use tokio::io::AsyncWriteExt;

struct SaveSession {
    final_path: PathBuf,
    part_path: PathBuf,
    file: tokio::fs::File,
}

static SAVE_SESSIONS: LazyLock<tokio::sync::Mutex<HashMap<String, SaveSession>>> =
    LazyLock::new(|| tokio::sync::Mutex::new(HashMap::new()));

/// Suffix of the in-progress sibling every save path writes to first
/// (DBR-06: 文件命名、覆盖确认、导出失败时原文件保留策略需统一).
/// A file picked in the save dialog is never truncated in place: `finish`
/// renames the sibling into it, so a failed or cancelled export leaves a
/// previously saved file untouched and only the sibling is cleaned up.
const PART_SUFFIX: &str = ".datazen-part";

fn part_sibling(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map(std::ffi::OsString::from)
        .unwrap_or_default();
    name.push(PART_SUFFIX);
    path.with_file_name(name)
}

/// Write `bytes` beside `path` first, then rename it into place.
///
/// The rename is the only step that touches the picked path, so a failure
/// mid-write (disk full, cancelled stream, crash) keeps any pre-existing file
/// intact instead of leaving a truncated half of it behind.
pub(crate) async fn write_all_via_part(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let part = part_sibling(path);
    tokio::fs::write(&part, bytes).await?;
    // `rename` replaces the target atomically on Unix but fails when the
    // target exists on Windows, so remove a pre-existing file first. The OS
    // save dialog has already asked for overwrite confirmation at this point.
    if tokio::fs::try_exists(path).await.unwrap_or(false) {
        tokio::fs::remove_file(path).await?;
    }
    tokio::fs::rename(&part, path).await
}

const ALLOWED_EXTENSIONS: &[&str] = &[
    "csv", "tsv", "json", "sql", "md", "txt", "xml", "yaml", "yml", "png", "svg", "zip", "gz",
    "dump", "xlsx",
    // DBR-05: the binary "load from file" of a BLOB column offers the common
    // binary payload types; every one of them is also a *pickable* file, so
    // the dialog filter and the extension check stay in one list.
    "jpg", "jpeg", "gif", "bmp", "webp", "ico", "pdf", "bin", "dat",
];

pub(crate) fn validate_extension(path: &Path, allowed: &[&str]) -> Result<(), CommandError> {
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
    let picked = super::dialog::save_file(
        &app,
        (
            filter_name,
            ext_list.iter().map(|s| (*s).to_string()).collect(),
        ),
        default_file_name,
    )
    .await?;
    let Some(path) = picked else {
        return Ok(false);
    };
    validate_extension(&path, &ext_list)?;
    write_all_via_part(&path, contents.as_bytes())
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
    let picked = super::dialog::save_file(
        &app,
        (
            filter_name,
            ext_list.iter().map(|s| (*s).to_string()).collect(),
        ),
        default_file_name,
    )
    .await?;
    let Some(path) = picked else {
        return Ok(false);
    };
    validate_extension(&path, &ext_list)?;
    write_all_via_part(&path, &bytes)
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
    let filters = vec![(
        filter_name,
        ext_list.iter().map(|s| (*s).to_string()).collect(),
    )];
    let picked = super::dialog::open_file(&app, filters).await?;
    let Some(path) = picked else {
        return Ok(None);
    };
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
    let filters = vec![(
        filter_name,
        ext_list.iter().map(|s| (*s).to_string()).collect(),
    )];
    let picked = super::dialog::open_file(&app, filters).await?;
    let Some(path) = picked else {
        return Ok(None);
    };
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
    // The session writes to the `.datazen-part` sibling; `finish_save` renames
    // it onto the picked path, so a pre-existing file survives every failure
    // mode (DBR-06: 导出失败时原文件保留).
    let part_path = part_sibling(&path);
    let file = tokio::fs::File::create(&part_path)
        .await
        .cmd_err("begin_save_with_dialog")?;
    let token = uuid::Uuid::new_v4().to_string();
    let mut sessions = SAVE_SESSIONS.lock().await;
    if sessions.len() >= 8 {
        return Err(CommandError::Validation(
            "Too many concurrent save sessions".into(),
        ));
    }
    sessions.insert(
        token.clone(),
        SaveSession {
            final_path: path,
            part_path,
            file,
        },
    );
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
    let picked = super::dialog::save_file(
        &app,
        (
            filter_name,
            ext_list.iter().map(|s| (*s).to_string()).collect(),
        ),
        default_file_name,
    )
    .await?;
    let Some(path) = picked else {
        return Ok(None);
    };
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

/// Close the session and move the finished sibling onto the picked path.
#[tauri::command]
pub async fn finish_save(token: String) -> Result<(), CommandError> {
    let mut sessions = SAVE_SESSIONS.lock().await;
    let session = sessions
        .remove(&token)
        .ok_or_else(|| CommandError::NotFound("Save session not found".into()))?;
    let SaveSession {
        final_path,
        part_path,
        mut file,
    } = session;
    let flush_result = file.flush().await;
    // The handle must be closed before the rename (Windows keeps an open file
    // from being replaced), so drop it even when the flush failed and report
    // the flush error afterwards.
    drop(file);
    if flush_result.is_err() {
        // The session is already off the map, so no `abort_save` will clean
        // this sibling up — remove it here rather than orphaning it.
        let _ = tokio::fs::remove_file(&part_path).await;
    }
    flush_result.cmd_err("finish_save")?;
    // Windows cannot rename onto an existing file; the save dialog already
    // collected the overwrite confirmation, so remove the old one first.
    if tokio::fs::try_exists(&final_path).await.unwrap_or(false) {
        tokio::fs::remove_file(&final_path)
            .await
            .cmd_err("finish_save")?;
    }
    tokio::fs::rename(&part_path, &final_path)
        .await
        .cmd_err("finish_save")?;
    Ok(())
}

/// Abort the session and delete its in-progress sibling.
///
/// Returns whether the sibling was actually removed, so the UI can report
/// "partial file deleted" vs "partial file kept" honestly (DBR-06: 取消
/// 后报告部分文件是否已删除或保留). The picked path itself is never touched
/// by an abort — a pre-existing file survives intact.
#[tauri::command]
pub async fn abort_save(token: String) -> Result<bool, CommandError> {
    let mut sessions = SAVE_SESSIONS.lock().await;
    let session = sessions
        .remove(&token)
        .ok_or_else(|| CommandError::NotFound("Save session not found".into()))?;
    drop(session.file);
    let deleted = tokio::fs::remove_file(&session.part_path).await.is_ok();
    Ok(deleted)
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
    fn ext_refs_rejects_unknown_extension() {
        let err = ext_refs(&["exe".into()]).unwrap_err();
        assert!(err.to_string().contains("not allowed"));
    }

    #[test]
    fn part_sibling_appends_suffix_to_file_name() {
        let part = part_sibling(Path::new("/tmp/dir/out.csv"));
        assert_eq!(part, Path::new("/tmp/dir/out.csv.datazen-part"));
    }

    #[tokio::test]
    async fn write_all_via_part_replaces_file_and_leaves_no_sibling() {
        let dir = std::env::temp_dir().join(format!("dz-part-{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(&dir).await.unwrap();
        let path = dir.join("out.txt");
        tokio::fs::write(&path, "OLD").await.unwrap();
        write_all_via_part(&path, b"NEW").await.unwrap();
        assert_eq!(tokio::fs::read_to_string(&path).await.unwrap(), "NEW");
        assert!(!part_sibling(&path).exists());
        let _ = tokio::fs::remove_dir_all(&dir).await;
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
        let deleted = abort_save(token).await.unwrap();
        assert!(deleted);
        assert!(!path.exists());
        let _ = tokio::fs::remove_dir_all(&dir).await;
    }

    /// DBR-06: a pre-existing file must survive a cancelled export — the
    /// session never writes onto the picked path, only onto its sibling.
    #[tokio::test]
    async fn save_session_abort_preserves_pre_existing_file() {
        let dir = std::env::temp_dir().join(format!("dz-abort-old-{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(&dir).await.unwrap();
        let path = dir.join("out.csv");
        tokio::fs::write(&path, "OLD").await.unwrap();
        let token = insert_save_session(path.clone()).await.unwrap();
        append_save_text(token.clone(), "NEW".into()).await.unwrap();
        let deleted = abort_save(token).await.unwrap();
        assert!(deleted);
        assert_eq!(tokio::fs::read_to_string(&path).await.unwrap(), "OLD");
        assert!(!part_sibling(&path).exists());
        let _ = tokio::fs::remove_dir_all(&dir).await;
    }

    /// DBR-06: `finish` replaces a pre-existing file only at the rename, and
    /// the in-progress sibling is gone afterwards.
    #[tokio::test]
    async fn save_session_finish_replaces_pre_existing_file() {
        let dir = std::env::temp_dir().join(format!("dz-finish-old-{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(&dir).await.unwrap();
        let path = dir.join("out.csv");
        tokio::fs::write(&path, "OLD").await.unwrap();
        let token = insert_save_session(path.clone()).await.unwrap();
        append_save_text(token.clone(), "NEW".into()).await.unwrap();
        finish_save(token).await.unwrap();
        assert_eq!(tokio::fs::read_to_string(&path).await.unwrap(), "NEW");
        assert!(!part_sibling(&path).exists());
        let _ = tokio::fs::remove_dir_all(&dir).await;
    }

    /// The abort outcome must be honest: if the sibling is already gone, the
    /// command says so instead of claiming a deletion.
    #[tokio::test]
    async fn save_session_abort_reports_false_when_sibling_already_gone() {
        let dir = std::env::temp_dir().join(format!("dz-abort-gone-{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(&dir).await.unwrap();
        let path = dir.join("out.md");
        let token = insert_save_session(path.clone()).await.unwrap();
        tokio::fs::remove_file(part_sibling(&path)).await.unwrap();
        let deleted = abort_save(token).await.unwrap();
        assert!(!deleted);
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
