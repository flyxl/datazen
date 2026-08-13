use super::error::{CmdExt, CommandError};
use crate::theme::surface_bg::{parse_css_hex, SurfaceBgCache};
use serde::Deserialize;
use tauri::webview::PageLoadEvent;
use tauri::window::Color;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

/// Built-in dark `--c-surface` / splash fallback (`#0f172a`).
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
    /// Last-resolved `--c-surface` hex from the opener (`#rgb` / `#rrggbb`).
    #[serde(default)]
    pub background_color: Option<String>,
}

fn default_title() -> String {
    "DataZen".into()
}
fn default_width() -> f64 {
    800.0
}
fn default_height() -> f64 {
    640.0
}
fn default_true() -> bool {
    true
}

fn parse_css_hex_color(s: &str) -> Option<Color> {
    parse_css_hex(s).map(|(r, g, b)| Color(r, g, b, 255))
}

fn window_background_color(override_hex: Option<&str>) -> Color {
    override_hex
        .and_then(parse_css_hex_color)
        .unwrap_or(WINDOW_BG_DARK)
}

fn resolved_window_background(app: &AppHandle, override_hex: Option<&str>) -> Color {
    let cached = app.try_state::<SurfaceBgCache>().map(|c| c.hex());
    window_background_color(cached.as_deref().or(override_hex))
}

pub const DOCS_WINDOW_LABEL: &str = "docs-singleton";

pub fn docs_window_options(section: Option<&str>) -> CreateWindowOptions {
    let mut qs = String::from("window=docs");
    if let Some(section) = section.map(str::trim).filter(|s| !s.is_empty()) {
        // Section ids are app-controlled (`overview`, `workflows`, …).
        qs.push_str("&section=");
        qs.push_str(section);
    }
    CreateWindowOptions {
        label: DOCS_WINDOW_LABEL.into(),
        url: format!("window.html?{qs}"),
        title: "DataZen".into(),
        width: 920.0,
        height: 680.0,
        min_width: Some(640.0),
        min_height: Some(480.0),
        center: true,
        accept_first_mouse: true,
        transparent: None,
        background_color: None,
    }
}

/// Open (or focus) the in-app docs singleton. Prefer calling this from Rust
/// menu handlers instead of `emit` → frontend → IPC round-trips.
pub async fn open_docs_window(app: AppHandle, section: Option<&str>) -> Result<(), CommandError> {
    create_sub_window(app, docs_window_options(section)).await
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
    .background_color(resolved_window_background(
        &app,
        options.background_color.as_deref(),
    ))
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
    let marker = url
        .strip_prefix("window.html")
        .or_else(|| url.strip_prefix("index.html"))
        .unwrap_or(url);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_window_options_defaults() {
        let json = r#"{"label":"w1","url":"window.html?window=settings"}"#;
        let opts: CreateWindowOptions = serde_json::from_str(json).unwrap();
        assert_eq!(opts.label, "w1");
        assert_eq!(opts.title, "DataZen");
        assert_eq!(opts.width, 800.0);
        assert_eq!(opts.height, 640.0);
        assert!(opts.center);
        assert!(opts.accept_first_mouse);
        assert!(opts.transparent.is_none());
        assert!(opts.background_color.is_none());
    }

    #[test]
    fn create_window_options_respects_overrides() {
        let json = r##"{
            "label":"w2",
            "url":"window.html?window=settings",
            "title":"Settings",
            "width":1024,
            "height":768,
            "center":false,
            "acceptFirstMouse":false,
            "transparent":true,
            "backgroundColor":"#1a0a2e"
        }"##;
        let opts: CreateWindowOptions = serde_json::from_str(json).unwrap();
        assert_eq!(opts.title, "Settings");
        assert_eq!(opts.width, 1024.0);
        assert!(!opts.center);
        assert!(!opts.accept_first_mouse);
        assert_eq!(opts.transparent, Some(true));
        assert_eq!(opts.background_color.as_deref(), Some("#1a0a2e"));
    }

    #[test]
    fn docs_window_options_are_singleton_with_docs_kind() {
        let opts = docs_window_options(None);
        assert_eq!(opts.label, DOCS_WINDOW_LABEL);
        assert_eq!(opts.url, "window.html?window=docs");
        assert_eq!(opts.width, 920.0);
        assert_eq!(opts.min_width, Some(640.0));
    }

    #[test]
    fn docs_window_options_append_section() {
        let opts = docs_window_options(Some("workflows"));
        assert_eq!(opts.label, DOCS_WINDOW_LABEL);
        assert_eq!(opts.url, "window.html?window=docs&section=workflows");
    }

    #[test]
    fn docs_window_options_ignore_blank_section() {
        let opts = docs_window_options(Some("  "));
        assert_eq!(opts.url, "window.html?window=docs");
    }

    #[test]
    fn parse_css_hex_color_accepts_3_and_6() {
        assert!(parse_css_hex_color("#0f172a").is_some());
        assert!(parse_css_hex_color("#fff").is_some());
        assert!(parse_css_hex_color("not-a-color").is_none());
        assert!(parse_css_hex_color("#gg0000").is_none());
        assert!(parse_css_hex_color("rgb(1,2,3)").is_none());
    }
}
