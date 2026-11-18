//! Layout, font, and window constants for the DataZen Rune UI.

pub(crate) const RAIL_WIDTH_COLLAPSED: u32 = 40;
pub(crate) const RAIL_WIDTH_EXPANDED: u32 = 112;
pub(crate) const NAVIGATOR_WIDTH_DEFAULT: u32 = 280;
pub(crate) const NAVIGATOR_WIDTH_MIN: u32 = 200;
pub(crate) const NAVIGATOR_WIDTH_MAX: u32 = 500;
pub(crate) const SIDEBAR_RAIL_WIDTH_KEY: &str = "sidebar-rail-width";
pub(crate) const NAVIGATOR_WIDTH_KEY: &str = "navigator-width";

/// Query panel layout (A3): editor height and the in-query sidebar width,
/// persisted through `Setting::Layout` like every other split.
pub(crate) const QUERY_EDITOR_HEIGHT_KEY: &str = "query-editor-height";
pub(crate) const QUERY_EDITOR_HEIGHT_DEFAULT: u32 = 320;
pub(crate) const QUERY_EDITOR_HEIGHT_MIN: u32 = 120;
pub(crate) const QUERY_EDITOR_HEIGHT_MAX: u32 = 900;
pub(crate) const QUERY_SIDEBAR_WIDTH_KEY: &str = "query-sidebar-width";
pub(crate) const QUERY_SIDEBAR_WIDTH_DEFAULT: u32 = 240;
pub(crate) const QUERY_SIDEBAR_WIDTH_MIN: u32 = 160;
pub(crate) const QUERY_SIDEBAR_WIDTH_MAX: u32 = 420;

/// The AI assistant aside (T009-S7): React's `useResizable` in
/// `ContentViewDrawers.tsx` (`initialSize: 320, minSize: 240, maxSize:
/// 600`, stored as `connection.aiSidebar`). Rune persists it through
/// `Setting::Layout` like every other split; the key differs because
/// the layout settings store is Rune's own, not React's storage key.
pub(crate) const AI_SIDEBAR_WIDTH_KEY: &str = "ai-sidebar-width";
pub(crate) const AI_SIDEBAR_WIDTH_DEFAULT: u32 = 320;
pub(crate) const AI_SIDEBAR_WIDTH_MIN: u32 = 240;
pub(crate) const AI_SIDEBAR_WIDTH_MAX: u32 = 600;

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
