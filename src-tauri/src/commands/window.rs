use super::error::{CmdExt, CommandError};
use serde::Deserialize;
use tauri::webview::PageLoadEvent;
use tauri::window::Color;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

/// Matches `index.html` dark splash / `documentElement` background (`#0f172a`).
const WINDOW_BG_DARK: Color = Color(0x0f, 0x17, 0x2a, 0xff);

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

/// Open (or focus) the in-app docs singleton. Prefer calling this from Rust
/// menu handlers instead of `emit` → frontend → IPC round-trips.
pub async fn open_docs_window(app: AppHandle, section: Option<&str>) -> Result<(), CommandError> {
    let mut qs = String::from("window=docs");
    if let Some(section) = section.map(str::trim).filter(|s| !s.is_empty()) {
        // Section ids are app-controlled (`overview`, `workflows`, …).
        qs.push_str("&section=");
        qs.push_str(section);
    }
    create_sub_window(
        app,
        CreateWindowOptions {
            label: "docs-singleton".into(),
            url: format!("index.html?{qs}"),
            title: "DataZen".into(),
            width: 920.0,
            height: 680.0,
            min_width: Some(640.0),
            min_height: Some(480.0),
            center: true,
            accept_first_mouse: true,
            transparent: None,
        },
    )
    .await
}

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
    if focus_existing_window(&app, &options.label, &options.url) {
        return Ok(());
    }

    let is_mac = cfg!(target_os = "macos");
    let transparent = options.transparent.unwrap_or(false);
    let label_for_log = options.label.clone();

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
    .background_color(WINDOW_BG_DARK)
    .accept_first_mouse(options.accept_first_mouse)
    // Show after HTML (theme + splash) has loaded — not immediately after
    // build() (white/light flash), and not only via frontend show() (ACL /
    // module load failures leave the window permanently invisible).
    .on_page_load(move |window, payload| {
        if payload.event() == PageLoadEvent::Finished {
            let _ = window.show();
            let _ = window.unminimize();
            let _ = window.set_focus();
            tracing::info!(label = %label_for_log, "sub window shown after page load");
        }
    });

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

    match builder.build() {
        Ok(_) => {
            tracing::info!(
                label = %options.label,
                url = %options.url,
                "sub window created (waiting for page load to show)"
            );
            Ok(())
        }
        Err(e) => {
            // Defense in depth: label may already be taken when multiple
            // webviews handled the same menu emit (historically TitleBar +
            // MainWindow both listened), or during a brief close/recreate
            // window. Prefer focusing the survivor over surfacing an error.
            let msg = e.to_string();
            if msg.to_ascii_lowercase().contains("already exists")
                && focus_existing_window(&app, &options.label, &options.url)
            {
                tracing::info!(
                    label = %options.label,
                    "create_sub_window recovered from already-exists"
                );
                return Ok(());
            }
            Err(CommandError::Internal(msg)).cmd_err("create_sub_window")
        }
    }
}

/// Show/focus an existing labeled window. Reloads when `url` query differs so
/// singleton reopen with a new query (e.g. docs `section`) takes effect.
fn focus_existing_window(app: &AppHandle, label: &str, url: &str) -> bool {
    let Some(existing) = app.get_webview_window(label) else {
        return false;
    };
    let marker = url.strip_prefix("index.html").unwrap_or(url);
    let needs_nav = existing
        .url()
        .map(|current| !current.as_str().contains(marker))
        .unwrap_or(true);
    if needs_nav {
        if let Ok(href) = serde_json::to_string(url) {
            let _ = existing.eval(format!("window.location.replace({href})"));
        }
    }
    let _ = existing.show();
    let _ = existing.unminimize();
    let _ = existing.set_focus();
    tracing::info!(label = %label, "create_sub_window reused existing");
    true
}
