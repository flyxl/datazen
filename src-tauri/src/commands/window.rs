use super::error::CommandError;
use serde::Deserialize;
use tauri::{AppHandle, WebviewUrl, WebviewWindowBuilder};

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
    pub decorations: Option<bool>,
    #[serde(default)]
    pub transparent: Option<bool>,
}

fn default_title() -> String { "DataZen".into() }
fn default_width() -> f64 { 800.0 }
fn default_height() -> f64 { 640.0 }
fn default_true() -> bool { true }

#[tauri::command]
pub fn create_sub_window(app: AppHandle, options: CreateWindowOptions) -> Result<(), CommandError> {
    let is_mac = cfg!(target_os = "macos");
    let decorations = options.decorations.unwrap_or(is_mac);
    let transparent = options.transparent.unwrap_or(false);

    let mut builder = WebviewWindowBuilder::new(
        &app,
        &options.label,
        WebviewUrl::App(options.url.into()),
    )
    .title(&options.title)
    .inner_size(options.width, options.height)
    .decorations(decorations)
    .transparent(transparent)
    .visible(false)
    .accept_first_mouse(options.accept_first_mouse);

    #[cfg(target_os = "macos")]
    {
        builder = builder
            .title_bar_style(tauri::TitleBarStyle::Overlay)
            .hidden_title(true)
            .traffic_light_position(tauri::LogicalPosition::new(14.0, 14.0));
    }

    if let Some(mw) = options.min_width {
        if let Some(mh) = options.min_height {
            builder = builder.min_inner_size(mw, mh);
        }
    }

    if options.center {
        builder = builder.center();
    }

    builder.build().map_err(|e| CommandError::Internal(e.to_string()))?;
    Ok(())
}
