use super::log_err;
use std::path::{Path, PathBuf};

#[tauri::command]
pub fn show_editor_context_menu(
    window: tauri::Window,
    lang: String,
) -> Result<(), String> {
    use tauri::menu::MenuBuilder;

    let label_favorite = if lang == "en" { "Add to Favorites" } else { "收藏 SQL" };

    let menu = MenuBuilder::new(&window)
        .text("ctx-add-favorite", label_favorite)
        .build()
        .map_err(|e| e.to_string())?;

    window.popup_menu(&menu).map_err(|e| e.to_string())?;

    Ok(())
}

const ALLOWED_EXTENSIONS: &[&str] = &["csv", "json", "sql", "md", "txt", "xml", "yaml", "yml"];

fn validate_file_path(path: &Path) -> Result<(), String> {
    let canonical = path.to_string_lossy();
    if canonical.contains("..") {
        return Err("Path traversal not allowed".to_string());
    }

    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        if !ALLOWED_EXTENSIONS.contains(&ext.to_lowercase().as_str()) {
            return Err(format!("File extension '.{}' not allowed", ext));
        }
    } else {
        return Err("File must have an extension".to_string());
    }

    Ok(())
}

#[tauri::command]
pub async fn write_file(path: String, contents: String) -> Result<(), String> {
    let p = PathBuf::from(&path);
    validate_file_path(&p)?;
    tokio::fs::write(&p, contents.as_bytes())
        .await
        .map_err(|e| log_err("write_file", &e))
}

#[tauri::command]
pub async fn read_file(path: String) -> Result<String, String> {
    let p = PathBuf::from(&path);
    validate_file_path(&p)?;
    tokio::fs::read_to_string(&p)
        .await
        .map_err(|e| log_err("read_file", &e))
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
        assert_eq!(err, "Path traversal not allowed");
    }

    #[test]
    fn validate_file_path_rejects_disallowed_extensions() {
        for path in ["malware.exe", "script.sh", "source.rs"] {
            let err = validate_file_path(Path::new(path)).unwrap_err();
            assert!(
                err.starts_with("File extension '.") && err.ends_with("' not allowed"),
                "unexpected error for {path}: {err}"
            );
        }
    }

    #[test]
    fn validate_file_path_rejects_missing_extension() {
        let err = validate_file_path(Path::new("/tmp/noext")).unwrap_err();
        assert_eq!(err, "File must have an extension");
    }
}
