use super::error::{CmdExt, CommandError};
use crate::theme::surface_bg::{parse_css_hex, SurfaceBgCache};
use crate::AppState;
use serde::Deserialize;
use tauri::webview::PageLoadEvent;
use tauri::window::Color;
use tauri::{
    AppHandle, LogicalSize, Manager, Size, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};

/// Main window defaults from `tauri.conf.json`.
const MAIN_WINDOW_DEFAULT_W: f64 = 1280.0;
const MAIN_WINDOW_DEFAULT_H: f64 = 820.0;
const MAIN_WINDOW_MIN_W: f64 = 960.0;
const MAIN_WINDOW_MIN_H: f64 = 640.0;
/// Legacy launcher size before the unified workspace shell.
const MAIN_WINDOW_LEGACY_W: f64 = 800.0;
const MAIN_WINDOW_LEGACY_H: f64 = 600.0;

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

const DOCS_BASE_EN: &str = "https://flyxl.github.io/datazen/manual.html";
const DOCS_BASE_ZH: &str = "https://flyxl.github.io/datazen/zh/manual.html";

/// Legacy section ids (from the removed docs.html) mapped to the closest
/// manual.html anchors. Mirrors `src/lib/docsUrls.ts`.
fn remap_section(section: &str) -> Option<&'static str> {
    match section {
        "overview" => Some("ui"),
        "features" => Some("charts"),
        "ai" | "context" => Some("ai"),
        "workflows" => Some("workflow"),
        "opsDashboard" => Some("dashboard"),
        "dataSync" => Some("data-sync"),
        "dataTransfer" => Some("data-transfer"),
        "schemaDiff" => Some("schema-diff"),
        _ => None,
    }
}

/// Official help docs URL (GitHub Pages). Mirrors `src/lib/docsUrls.ts`.
pub fn docs_url(language: &str, section: Option<&str>) -> String {
    let base = if language.starts_with("zh") {
        DOCS_BASE_ZH
    } else {
        DOCS_BASE_EN
    };
    if let Some(section) = section.map(str::trim).filter(|s| !s.is_empty()) {
        if let Some(anchor) = remap_section(section) {
            return format!("{base}#{anchor}");
        }
    }
    base.to_string()
}

/// Open official help docs in the system browser (native Help menu).
pub async fn open_docs_window(app: AppHandle, section: Option<&str>) -> Result<(), CommandError> {
    let state = app.state::<AppState>();
    let settings = state.store.get_settings().await;
    let url = docs_url(&settings.language, section);
    open::that(&url).map_err(|e| CommandError::Internal(format!("open_docs_window: {e}")))
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

pub(crate) fn main_window_needs_default_size(logical_width: f64, logical_height: f64) -> bool {
    logical_width < MAIN_WINDOW_MIN_W
        || logical_height < MAIN_WINDOW_MIN_H
        || ((logical_width - MAIN_WINDOW_LEGACY_W).abs() < 1.0
            && (logical_height - MAIN_WINDOW_LEGACY_H).abs() < 1.0)
}

/// Resize when macOS restores a legacy frame below minimum bounds.
pub fn prepare_main_window(window: &WebviewWindow) {
    ensure_main_window_size(window);
}

/// Resize the main window when macOS restores an old 800×600 frame or a size below min bounds.
fn ensure_main_window_size(window: &WebviewWindow) {
    let Ok(size) = window.inner_size() else {
        return;
    };
    let scale = window.scale_factor().unwrap_or(1.0);
    let logical_w = size.width as f64 / scale;
    let logical_h = size.height as f64 / scale;
    if !main_window_needs_default_size(logical_w, logical_h) {
        return;
    }
    let _ = window.set_size(Size::Logical(LogicalSize::new(
        MAIN_WINDOW_DEFAULT_W,
        MAIN_WINDOW_DEFAULT_H,
    )));
    tracing::info!(
        from_w = logical_w,
        from_h = logical_h,
        to_w = MAIN_WINDOW_DEFAULT_W,
        to_h = MAIN_WINDOW_DEFAULT_H,
        "main window resized to configured default"
    );
}

/// Labels of open windows excluding the main window.
pub fn non_main_window_labels<I, S>(labels: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    labels
        .into_iter()
        .map(|l| l.as_ref().to_string())
        .filter(|l| l != "main")
        .collect()
}

/// Whether a main-window close request should be blocked and minimized instead
/// because other webview windows are still open.
pub fn main_close_blocked_by_child_windows<I, S>(labels: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    !non_main_window_labels(labels).is_empty()
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

/// Singleton migration tool sub-windows opened from the native Tools menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationSubWindow {
    DataSync,
    DataTransfer,
    SchemaDiff,
    Backup,
    Restore,
}

impl MigrationSubWindow {
    fn spec(self) -> (&'static str, &'static str, f64, f64, f64, f64) {
        match self {
            Self::DataSync => (
                "data-sync-singleton",
                "window.html?window=data-sync",
                1000.0,
                700.0,
                600.0,
                480.0,
            ),
            Self::DataTransfer => (
                "data-transfer-singleton",
                "window.html?window=data-transfer",
                1000.0,
                720.0,
                640.0,
                480.0,
            ),
            Self::SchemaDiff => (
                "schema-diff-singleton",
                "window.html?window=schema-diff",
                900.0,
                640.0,
                560.0,
                420.0,
            ),
            Self::Backup => (
                "backup-singleton",
                "window.html?window=backup",
                750.0,
                520.0,
                600.0,
                400.0,
            ),
            Self::Restore => (
                "backup-restore-singleton",
                "window.html?window=backup&mode=restore",
                750.0,
                520.0,
                600.0,
                400.0,
            ),
        }
    }

    fn menu_title_key(self) -> &'static str {
        match self {
            Self::DataSync => "data-sync",
            Self::DataTransfer => "data-transfer",
            Self::SchemaDiff => "schema-diff",
            Self::Backup => "backup",
            Self::Restore => "restore",
        }
    }
}

/// Open (or focus) a migration/backup singleton from the native menu — one Rust
/// handler avoids duplicate JS listeners across multiple webviews.
pub async fn open_migration_sub_window(
    app: AppHandle,
    kind: MigrationSubWindow,
) -> Result<(), CommandError> {
    let state = app.state::<AppState>();
    let settings = state.store.get_settings().await;
    let menu_title = crate::menu_label(&settings.language, kind.menu_title_key());
    let title = format!("{menu_title} - DataZen");
    let (label, url, width, height, min_width, min_height) = kind.spec();

    create_sub_window(
        app,
        CreateWindowOptions {
            label: label.into(),
            url: url.into(),
            title,
            width,
            height,
            min_width: Some(min_width),
            min_height: Some(min_height),
            center: true,
            accept_first_mouse: true,
            transparent: None,
            background_color: None,
        },
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn main_window_needs_default_size_for_legacy_and_too_small() {
        assert!(main_window_needs_default_size(800.0, 600.0));
        assert!(main_window_needs_default_size(900.0, 700.0));
        assert!(!main_window_needs_default_size(1280.0, 820.0));
        assert!(!main_window_needs_default_size(1440.0, 900.0));
    }

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
    fn docs_url_uses_english_base_by_default() {
        assert_eq!(docs_url("en", None), DOCS_BASE_EN);
        assert_eq!(docs_url("de", Some("ai")), format!("{DOCS_BASE_EN}#ai"));
    }

    #[test]
    fn docs_url_uses_chinese_base_for_zh_locales() {
        assert_eq!(docs_url("zh-CN", None), DOCS_BASE_ZH);
        assert_eq!(
            docs_url("zh-TW", Some("workflows")),
            format!("{DOCS_BASE_ZH}#workflow")
        );
    }

    #[test]
    fn docs_url_remaps_legacy_sections_to_manual_anchors() {
        let cases = [
            ("overview", "ui"),
            ("features", "charts"),
            ("ai", "ai"),
            ("context", "ai"),
            ("workflows", "workflow"),
            ("opsDashboard", "dashboard"),
            ("dataSync", "data-sync"),
            ("dataTransfer", "data-transfer"),
            ("schemaDiff", "schema-diff"),
        ];
        for (legacy, anchor) in cases {
            assert_eq!(
                docs_url("en", Some(legacy)),
                format!("{DOCS_BASE_EN}#{anchor}")
            );
            assert_eq!(
                docs_url("zh-CN", Some(legacy)),
                format!("{DOCS_BASE_ZH}#{anchor}")
            );
        }
    }

    #[test]
    fn docs_url_ignores_unknown_sections() {
        assert_eq!(docs_url("en", Some("getting-started")), DOCS_BASE_EN);
        assert_eq!(docs_url("en", Some("  ")), DOCS_BASE_EN);
    }

    #[test]
    fn non_main_window_labels_drops_main_only() {
        let labels = non_main_window_labels(["main", "backup-singleton", "connection-1"]);
        assert_eq!(labels, vec!["backup-singleton", "connection-1"]);
        assert!(non_main_window_labels(["main"]).is_empty());
    }

    #[test]
    fn main_close_blocked_when_child_windows_open() {
        assert!(!main_close_blocked_by_child_windows(["main"]));
        assert!(main_close_blocked_by_child_windows([
            "main",
            "backup-singleton"
        ]));
        assert!(main_close_blocked_by_child_windows([
            "main",
            "data-sync-singleton",
        ]));
        assert!(main_close_blocked_by_child_windows([
            "main",
            "schema-diff-singleton",
            "data-transfer-singleton",
        ]));
    }

    #[test]
    fn migration_sub_window_specs_match_frontend_singleton_labels() {
        use super::MigrationSubWindow;
        assert_eq!(
            MigrationSubWindow::DataSync.spec(),
            (
                "data-sync-singleton",
                "window.html?window=data-sync",
                1000.0,
                700.0,
                600.0,
                480.0
            )
        );
        assert_eq!(
            MigrationSubWindow::DataTransfer.spec(),
            (
                "data-transfer-singleton",
                "window.html?window=data-transfer",
                1000.0,
                720.0,
                640.0,
                480.0
            )
        );
        assert_eq!(
            MigrationSubWindow::SchemaDiff.spec(),
            (
                "schema-diff-singleton",
                "window.html?window=schema-diff",
                900.0,
                640.0,
                560.0,
                420.0
            )
        );
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
