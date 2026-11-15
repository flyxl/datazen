//! Layout, font, and window constants for the DataZen Rune UI.

pub(crate) const RAIL_WIDTH_COLLAPSED: u32 = 40;
pub(crate) const RAIL_WIDTH_EXPANDED: u32 = 112;
pub(crate) const NAVIGATOR_WIDTH_DEFAULT: u32 = 280;
pub(crate) const NAVIGATOR_WIDTH_MIN: u32 = 200;
pub(crate) const NAVIGATOR_WIDTH_MAX: u32 = 500;
pub(crate) const SIDEBAR_RAIL_WIDTH_KEY: &str = "sidebar-rail-width";
pub(crate) const NAVIGATOR_WIDTH_KEY: &str = "navigator-width";

/// The app's font pool (H8): rune's own Inter faces keep the UI's Latin look
/// and the web's sans fallback, IBM Plex Mono keeps the editor's mono slot,
/// and Noto Sans SC covers Chinese in the interface and the grid — the sans
/// slot lists it after Inter, so Han text falls back to it glyph by glyph.
/// Replaces rune's built-in set (AppSpec.fonts semantics) so every platform,
/// the web included, draws the same text.
pub(crate) const APP_FONTS: &[&[u8]] = &[
    include_bytes!("../assets/Inter-Regular.ttf"),
    include_bytes!("../assets/Inter-Medium.ttf"),
    include_bytes!("../assets/Inter-SemiBold.ttf"),
    include_bytes!("../assets/NotoSansSC-Regular.ttf"),
    include_bytes!("../assets/IBMPlexMono-Regular.ttf"),
];
pub(crate) const APP_FONT_FAMILIES: (&str, &str) = ("Inter, Noto Sans SC", "IBM Plex Mono");
pub const TABLE_PAGE_SIZE: u32 = 25;

/// The app's window chrome (L3): self-drawn 40pt titlebar and a minimum size
/// aligned with the React/Tauri main window.
pub(crate) const TITLEBAR_HEIGHT: f32 = 40.0;
pub(crate) const WINDOW_MIN_SIZE: (f32, f32) = (960.0, 640.0);
