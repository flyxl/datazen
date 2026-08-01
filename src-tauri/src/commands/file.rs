use super::error::{CmdExt, CommandError};
use std::path::{Path, PathBuf};

#[tauri::command]
pub fn show_editor_context_menu(
    window: tauri::Window,
    lang: String,
) -> Result<(), CommandError> {
    use tauri::menu::MenuBuilder;

    let label_favorite = if lang == "en" { "Add to Favorites" } else { "收藏 SQL" };

    let menu = MenuBuilder::new(&window)
        .text("ctx-add-favorite", label_favorite)
        .build()
        .map_err(|e| CommandError::Internal(e.to_string()))?;

    window.popup_menu(&menu).map_err(|e| CommandError::Internal(e.to_string()))?;

    Ok(())
}

const ALLOWED_EXTENSIONS: &[&str] = &["csv", "json", "sql", "md", "txt", "xml", "yaml", "yml"];

fn validate_file_path(path: &Path) -> Result<(), CommandError> {
    let canonical = path.to_string_lossy();
    if canonical.contains("..") {
        return Err(CommandError::Validation("Path traversal not allowed".into()));
    }

    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        if !ALLOWED_EXTENSIONS.contains(&ext.to_lowercase().as_str()) {
            return Err(CommandError::Validation(format!("File extension '.{}' not allowed", ext)));
        }
    } else {
        return Err(CommandError::Validation("File must have an extension".into()));
    }

    Ok(())
}

#[tauri::command]
pub async fn write_file(path: String, contents: String) -> Result<(), CommandError> {
    let p = PathBuf::from(&path);
    validate_file_path(&p)?;
    tokio::fs::write(&p, contents.as_bytes())
        .await
        .cmd_err("write_file")
}

#[tauri::command]
pub async fn read_file(path: String) -> Result<String, CommandError> {
    let p = PathBuf::from(&path);
    validate_file_path(&p)?;
    tokio::fs::read_to_string(&p)
        .await
        .cmd_err("read_file")
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
