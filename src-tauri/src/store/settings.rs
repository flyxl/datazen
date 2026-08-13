use crate::dashboard::types::MonitorSettings;
use crate::mcp::permission::McpPermissionMode;
use serde::{Deserialize, Serialize};

/// Light / dark / system mode plus optional installed theme pack.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ThemePreference {
    pub mode: String,
    #[serde(default)]
    pub pack_id: Option<String>,
}

impl Default for ThemePreference {
    fn default() -> Self {
        Self {
            mode: "dark".into(),
            pack_id: None,
        }
    }
}

pub(crate) fn deserialize_theme<'de, D>(deserializer: D) -> Result<ThemePreference, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::String(s) if matches!(s.as_str(), "light" | "dark" | "system") => {
            Ok(ThemePreference {
                mode: s,
                pack_id: None,
            })
        }
        other => serde_json::from_value(other).map_err(serde::de::Error::custom),
    }
}

/// Application settings persisted on disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    #[serde(deserialize_with = "deserialize_theme", default)]
    pub theme: ThemePreference,
    pub language: String,
    #[serde(default = "default_limit_select")]
    pub limit_select_results: bool,
    pub query_result_limit: u32,
    pub editor_font_size: u32,
    pub editor_font_family: String,
    pub confirm_on_delete: bool,
    pub auto_commit: bool,
    /// Require WHERE on UPDATE/DELETE; also block TRUNCATE/DROP (TablePlus-style Safe Mode). Default on.
    #[serde(default = "default_true")]
    pub safe_mode: bool,
    pub default_page_size: u32,
    /// Max connections per DB session pool (Postgres/MySQL). Applies on next connect.
    #[serde(default = "default_connection_pool_size")]
    pub connection_pool_size: u32,
    #[serde(default = "default_log_level")]
    pub log_level: String,
    #[serde(default)]
    pub log_path: String,
    /// When true, GUI may start an embedded MCP stdio server on launch.
    /// Default false — MCP for external clients should use `datazen --mcp`.
    #[serde(default)]
    pub mcp_server_enabled: bool,
    #[serde(default)]
    pub mcp_disabled_tools: Vec<String>,
    /// MCP tool permission tier for external AI clients (default: safe_write).
    #[serde(default)]
    pub mcp_permission_mode: McpPermissionMode,
    /// Persistent connection config IDs exposed to MCP. Empty = all connections.
    #[serde(default)]
    pub mcp_allowed_connection_ids: Vec<String>,
    #[serde(default)]
    pub context_dir: String,
    /// When true, GUI checks for app updates on startup (default off).
    #[serde(default)]
    pub check_for_updates_on_startup: bool,
    /// After a successful query, switch to chart view when the result is chartable.
    #[serde(default = "default_true")]
    pub auto_chart_on_query: bool,
    /// Dashboard monitor / tray / retention settings (nested for settings UI).
    #[serde(default)]
    pub monitor: MonitorSettings,
    /// Opaque per-plugin settings keyed by plugin id (e.g. `"redis"`).
    #[serde(default)]
    pub plugin_settings: serde_json::Map<String, serde_json::Value>,
}

fn default_limit_select() -> bool {
    false
}

fn default_true() -> bool {
    true
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_connection_pool_size() -> u32 {
    10
}

/// Clamp user-facing pool size to a safe range.
pub fn clamp_connection_pool_size(n: u32) -> u32 {
    n.clamp(1, 100)
}

impl AppSettings {
    /// Defaults used on first install when `settings.json` is absent.
    pub fn default_for_first_run() -> Self {
        let mut settings = Self::default();
        settings.language = crate::i18n_locale::default_ui_language();
        settings
    }
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: ThemePreference::default(),
            language: "en".to_string(),
            limit_select_results: false,
            query_result_limit: 5000,
            editor_font_size: 13,
            editor_font_family: "JetBrains Mono".to_string(),
            confirm_on_delete: true,
            auto_commit: true,
            safe_mode: true,
            default_page_size: 50,
            connection_pool_size: default_connection_pool_size(),
            log_level: default_log_level(),
            log_path: String::new(),
            mcp_server_enabled: false,
            mcp_disabled_tools: Vec::new(),
            mcp_permission_mode: McpPermissionMode::default(),
            mcp_allowed_connection_ids: Vec::new(),
            context_dir: String::new(),
            check_for_updates_on_startup: false,
            auto_chart_on_query: true,
            monitor: MonitorSettings::default(),
            plugin_settings: serde_json::Map::new(),
        }
    }
}
