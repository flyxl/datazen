use crate::dashboard::types::MonitorSettings;
use crate::mcp::permission::McpPermissionMode;
use crate::mcp::McpServerConfig;
use serde::{Deserialize, Serialize};

/// Current first-run journey revision. Bump it only when the journey must be
/// shown again to users who already finished an earlier revision.
pub const ONBOARDING_VERSION: i32 = 1;

/// Onboarding wizard completion state.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OnboardingState {
    pub completed: bool,
    pub version: i32,
}

impl OnboardingState {
    /// Fresh installation: the journey has never been shown.
    pub fn for_fresh_install() -> Self {
        Self {
            completed: false,
            version: ONBOARDING_VERSION,
        }
    }

    /// Existing installation that predates the journey: mark it as seen so an
    /// upgrading user is never onboarded (requirement: upgrade ≠ first run).
    pub fn for_existing_install() -> Self {
        Self {
            completed: true,
            version: ONBOARDING_VERSION,
        }
    }
}

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
    /// When Safe Mode is off, prompt before executing high-risk/production SQL. Default on.
    #[serde(default = "default_true")]
    pub confirm_dangerous_execution: bool,
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
    #[serde(default)]
    pub auto_chart_on_query: bool,
    /// Dashboard monitor / tray / retention settings (nested for settings UI).
    #[serde(default)]
    pub monitor: MonitorSettings,
    /// Opaque per-driver settings keyed by driver id (e.g. `"redis"`).
    #[serde(default, alias = "plugin_settings", alias = "wapp_settings")]
    pub driver_settings: serde_json::Map<String, serde_json::Value>,
    /// Opaque per-wapp settings keyed by wapp id. Currently unused, reserved for future workspace app configs.
    #[serde(default)]
    pub wapp_settings: serde_json::Map<String, serde_json::Value>,
    /// Saved external MCP Client server configs (stdio). Runtime connections are separate.
    #[serde(default)]
    pub mcp_client_servers: Vec<McpServerConfig>,
    /// When true (default), strip query result rows and payloads before AI requests leave the device.
    #[serde(default = "default_true")]
    pub ai_strict_egress: bool,
    /// Identifier quotation policy for SQL completion ('unquoted' | 'always' | 'both'). Default 'unquoted'.
    #[serde(default = "default_completion_quote_policy")]
    pub editor_completion_quote_policy: String,
    /// Keyboard shortcut preset ('default' | 'dbeaver' | 'navicat').
    #[serde(default = "default_keymap_preset")]
    pub keymap_preset: String,
    /// User-customized keyboard shortcut overrides keyed by action ID.
    #[serde(default)]
    pub custom_keymap: std::collections::HashMap<String, String>,
    /// SQL execution strategy ('entire_script' | 'current_statement' | 'largest_statement' | 'ask').
    #[serde(default = "default_sql_execution_strategy")]
    pub sql_execution_strategy: String,
    /// SQL syntax highlighting color preset ('default' follows the active theme pack).
    #[serde(default)]
    pub sql_syntax_theme: Option<String>,
    /// Onboarding wizard state. `None` or `version < 1` → show wizard.
    #[serde(default)]
    pub onboarding: Option<OnboardingState>,
}

fn default_sql_execution_strategy() -> String {
    "entire_script".to_string()
}

fn default_completion_quote_policy() -> String {
    "unquoted".to_string()
}

fn default_keymap_preset() -> String {
    "default".to_string()
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
    /// Defaults used on a brand-new installation when `settings.json` is absent.
    ///
    /// The onboarding state is materialized as "not completed" so the first-run
    /// journey is shown. Writing it immediately also keeps the very next launch
    /// from mistaking this install for an upgrade (see [`crate::store::Store::load_all`]).
    pub fn default_for_first_run() -> Self {
        let mut settings = Self::default();
        settings.language = crate::i18n_locale::default_ui_language();
        settings.onboarding = Some(OnboardingState::for_fresh_install());
        settings
    }

    /// Resolve the onboarding state of a settings file that was already on disk.
    ///
    /// A file without the `onboarding` key was written by a build that predates
    /// the first-run journey → the user is upgrading, so the journey stays
    /// hidden. Returns `true` when the resolved state must be persisted.
    pub fn resolve_loaded_onboarding(&mut self) -> bool {
        if self.onboarding.is_some() {
            return false;
        }
        self.onboarding = Some(OnboardingState::for_existing_install());
        true
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
            confirm_dangerous_execution: true,
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
            auto_chart_on_query: false,
            monitor: MonitorSettings::default(),
            driver_settings: serde_json::Map::new(),
            wapp_settings: serde_json::Map::new(),
            mcp_client_servers: Vec::new(),
            ai_strict_egress: true,
            editor_completion_quote_policy: default_completion_quote_policy(),
            keymap_preset: default_keymap_preset(),
            custom_keymap: std::collections::HashMap::new(),
            sql_execution_strategy: default_sql_execution_strategy(),
            sql_syntax_theme: None,
            onboarding: None,
        }
    }
}
