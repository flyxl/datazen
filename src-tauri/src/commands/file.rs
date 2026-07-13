use super::log_err;
use std::path::PathBuf;

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
#[tauri::command]
pub async fn write_file(path: String, contents: String) -> Result<(), String> {
    let p = PathBuf::from(&path);
    tokio::fs::write(&p, contents.as_bytes())
        .await
        .map_err(|e| log_err("write_file", &e))
}

#[tauri::command]
pub async fn read_file(path: String) -> Result<String, String> {
    let p = PathBuf::from(&path);
    tokio::fs::read_to_string(&p)
        .await
        .map_err(|e| log_err("read_file", &e))
}
