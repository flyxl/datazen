use super::error::{CmdExt, CommandError};
use serde::Deserialize;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateWindowOptions {
    pub label: String,
    pub url: String,
    #[serde(default = "default_title")]
    pub title: String,
    #[serde(default = "default_width")]
    pub width: f64,
    #[serde(default = "default_height")]
    pub height: f64,
    pub min_width: Option<f64>,
    pub min_height: Option<f64>,
    #[serde(default = "default_true")]
    pub center: bool,
    #[serde(default = "default_true")]
    pub accept_first_mouse: bool,
    #[serde(default)]
    pub transparent: Option<bool>,
}

fn default_title() -> String { "DataZen".into() }
fn default_width() -> f64 { 800.0 }
fn default_height() -> f64 { 640.0 }
fn default_true() -> bool { true }

/// Create (or focus) a sub-window.
///
/// **Must be `async` on Windows.** Tauri/WebView2 deadlocks when
/// `WebviewWindowBuilder::build()` is called from a synchronous command
/// handler — the app freezes right after "open window" and never reaches
/// the post-build log line. See Tauri docs on `WebviewWindowBuilder::build`.
#[tauri::command]
pub async fn create_sub_window(
    app: AppHandle,
    options: CreateWindowOptions,
) -> Result<(), CommandError> {
    tracing::info!(label = %options.label, url = %options.url, "create_sub_window requested");

    // Singleton windows are requested repeatedly. Reuse here (not only in
    // JS) so a window that exists but stayed hidden can still be shown.
    if let Some(existing) = app.get_webview_window(&options.label) {
        let _ = existing.show();
        let _ = existing.unminimize();
        let _ = existing.set_focus();
        tracing::info!(label = %options.label, "create_sub_window reused existing");
        return Ok(());
    }

    let is_mac = cfg!(target_os = "macos");
    let transparent = options.transparent.unwrap_or(false);

    let mut builder = WebviewWindowBuilder::new(
        &app,
        &options.label,
        WebviewUrl::App(options.url.clone().into()),
    )
    .title(&options.title)
    .inner_size(options.width, options.height)
    .decorations(is_mac)
    .transparent(transparent)
    .visible(false)
    .accept_first_mouse(options.accept_first_mouse);

    #[cfg(target_os = "macos")]
    {
        builder = builder
            .title_bar_style(tauri::TitleBarStyle::Overlay)
            .hidden_title(true)
            .traffic_light_position(tauri::LogicalPosition::new(16.0, 18.0));
    }

    if let Some(mw) = options.min_width {
        if let Some(mh) = options.min_height {
            builder = builder.min_inner_size(mw, mh);
        }
    }

    if options.center {
        builder = builder.center();
    }

    let window = builder
        .build()
        .map_err(|e| CommandError::Internal(e.to_string()))
        .cmd_err("create_sub_window")?;

    // Show from Rust (same as the reuse path). Frontend `main.tsx` also calls
    // show(), but that requires the new label to be listed in capabilities —
    // missing ACL previously left windows like `docs-singleton` invisible.
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();

    tracing::info!(label = %options.label, url = %options.url, "sub window created");
    Ok(())
}
