mod flows;
mod normalize;

pub use flows::{
    authoritative_read, load_fence, load_fence_current, persist_merge, settings_changed,
    theme_recovery, update_settings_merge, value_presentation_fresh, LoadRecovery,
};
pub use normalize::{
    is_unknown_version_value_presentation, next_value_presentation_revision,
    normalize_loaded_settings, normalize_settings_language, parse_stored_value_presentation,
    resolve_ui_language,
};

pub const SETTINGS_THEME_STORAGE_KEY: &str = "datazen-theme";
pub const SETTINGS_CHANGED_EVENT: &str = "datazen:settings-changed";
pub const THEME_CHANGED_EVENT: &str = "datazen:theme-changed";
pub const VALUE_PRESENTATION_VERSION: u64 = 1;

pub const DEFAULT_LANGUAGE: &str = "en";
pub const DEFAULT_CONNECTION_POOL_SIZE: u64 = 10;
pub const DEFAULT_QUERY_RESULT_LIMIT: u64 = 5000;
pub const DEFAULT_EDITOR_FONT_SIZE: u64 = 13;
pub const DEFAULT_PAGE_SIZE: u64 = 50;
pub const DEFAULT_MCP_PERMISSION_MODE: &str = "safe_write";
pub const DEFAULT_LOG_LEVEL: &str = "info";

pub const SETTINGS_STORE_SURFACE: [&str; 10] = [
    "loadSettings",
    "updateSettings",
    "updateValuePresentationSettings",
    "applySettingsLocally",
    "applyThemeLocally",
    "normalizeLoadedSettings",
    "readAuthoritativeSettings",
    "nextValuePresentationRevision",
    "isUnknownVersionValuePresentation",
    "SETTINGS_STORE_GAP",
];

pub const SETTINGS_STORE_GAP: [&str; 9] = [
    "Zustand store creation and setState wiring",
    "settingsCommands getSettings / saveSettings / syncLanguageMenu IPC",
    "crossWindowBus emit and useSettingsSync authoritative reread",
    "theme application, matchMedia watcher and document classList",
    "localStorage for the persisted theme mode",
    "navigator.language for the first-load language fallback",
    "bindSettingsStore injection into the driver SDK",
    "applyValuePresentationUpdate path-scoped updaters",
    "load generation counters and async IPC timing",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_surfaces_and_gaps_are_pinned() {
        assert_eq!(SETTINGS_THEME_STORAGE_KEY, "datazen-theme");
        assert_eq!(SETTINGS_CHANGED_EVENT, "datazen:settings-changed");
        assert_eq!(THEME_CHANGED_EVENT, "datazen:theme-changed");
        assert_eq!(DEFAULT_CONNECTION_POOL_SIZE, 10);
        assert_eq!(SETTINGS_STORE_SURFACE.len(), 10);
        assert_eq!(SETTINGS_STORE_GAP.len(), 9);
        assert_eq!(crate::SETTINGS_STORE_SURFACE.len(), 10);
        assert_eq!(crate::SETTINGS_STORE_GAP.len(), 9);
    }
}
