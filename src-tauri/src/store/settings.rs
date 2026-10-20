use crate::dashboard::types::MonitorSettings;
use crate::mcp::permission::McpPermissionMode;
use crate::mcp::McpServerConfig;
use datazen_platform_api::settings::{ExecutionSettings, SharedSettings, UserPreferences};
pub use datazen_platform_api::settings::{SqlFormatOptions, SqlSnippet};
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
    /// Directory holding the user's favorites, one `.sql` file each.
    ///
    /// Empty/`None` means `{appData}/favorites`. The default is **not** inside
    /// a cloud drive — on macOS and Windows the platform app-data directory
    /// does not sync — so this field is what makes cross-device favorites
    /// possible: point it at an iCloud/Dropbox/GitHub folder and the files
    /// travel (plan §2.6.3).
    #[serde(default)]
    pub favorites_root: Option<String>,
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
    /// Automatically qualify column completions. Default true for older settings.
    #[serde(default = "default_true")]
    pub editor_completion_include_table_prefix: bool,
    /// Infer foreign keys from table structure and naming when the schema declares
    /// none, and use them for JOIN suggestions, completion ranking and the ER
    /// diagram. Off by default: a prediction is a guess, so it is opt-in. It can be
    /// switched on in Settings → Editor or from the ER diagram's legend, and a
    /// prediction is always visually distinct from a declared constraint.
    #[serde(default)]
    pub enable_fk_prediction: bool,
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
    #[serde(default)]
    pub sql_format_options: SqlFormatOptions,
    #[serde(default)]
    pub sql_snippets: Vec<SqlSnippet>,
    #[serde(default = "default_workflow_step_result_order")]
    pub workflow_step_result_order: String,
    /// Preserve future/extension fields when an older desktop writes the file.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
    /// Onboarding wizard state. `None` or `version < 1` → show wizard.
    #[serde(default)]
    pub onboarding: Option<OnboardingState>,
}

fn default_workflow_step_result_order() -> String {
    "desc".into()
}

fn default_sql_execution_strategy() -> String {
    "current_statement".to_string()
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
            editor_font_family: "Menlo, Monaco, Consolas, \"Courier New\", monospace".to_string(),
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
            favorites_root: None,
            check_for_updates_on_startup: false,
            auto_chart_on_query: false,
            monitor: MonitorSettings::default(),
            driver_settings: serde_json::Map::new(),
            wapp_settings: serde_json::Map::new(),
            mcp_client_servers: Vec::new(),
            ai_strict_egress: true,
            editor_completion_quote_policy: default_completion_quote_policy(),
            editor_completion_include_table_prefix: true,
            enable_fk_prediction: false,
            keymap_preset: default_keymap_preset(),
            custom_keymap: std::collections::HashMap::new(),
            sql_execution_strategy: default_sql_execution_strategy(),
            sql_syntax_theme: None,
            sql_format_options: SqlFormatOptions::default(),
            sql_snippets: Vec::new(),
            workflow_step_result_order: default_workflow_step_result_order(),
            extra: serde_json::Map::new(),
            onboarding: None,
        }
    }
}

impl AppSettings {
    pub fn shared_settings(&self) -> SharedSettings {
        SharedSettings {
            preferences: UserPreferences {
                theme: datazen_platform_api::settings::PortableThemePreference {
                    mode: self.theme.mode.clone(),
                    pack_id: self.theme.pack_id.clone(),
                },
                language: self.language.clone(),
                editor_font_size: self.editor_font_size.clone(),
                editor_font_family: self.editor_font_family.clone(),
                confirm_on_delete: self.confirm_on_delete.clone(),
                auto_commit: self.auto_commit.clone(),
                confirm_dangerous_execution: self.confirm_dangerous_execution.clone(),
                default_page_size: self.default_page_size.clone(),
                auto_chart_on_query: self.auto_chart_on_query.clone(),
                wapp_settings: self.wapp_settings.clone(),
                editor_completion_quote_policy: self.editor_completion_quote_policy.clone(),
                editor_completion_include_table_prefix: self
                    .editor_completion_include_table_prefix
                    .clone(),
                enable_fk_prediction: self.enable_fk_prediction.clone(),
                keymap_preset: self.keymap_preset.clone(),
                custom_keymap: self.custom_keymap.clone(),
                sql_execution_strategy: self.sql_execution_strategy.clone(),
                sql_syntax_theme: self.sql_syntax_theme.clone(),
                sql_format_options: self.sql_format_options.clone(),
                sql_snippets: self.sql_snippets.clone(),
                workflow_step_result_order: self.workflow_step_result_order.clone(),
            },
            execution: ExecutionSettings {
                limit_select_results: self.limit_select_results.clone(),
                query_result_limit: self.query_result_limit.clone(),
                safe_mode: self.safe_mode.clone(),
                connection_pool_size: self.connection_pool_size.clone(),
                mcp_disabled_tools: self.mcp_disabled_tools.clone(),
                mcp_permission_mode: match self.mcp_permission_mode {
                    McpPermissionMode::ReadOnly => "read_only",
                    McpPermissionMode::SafeWrite => "safe_write",
                    McpPermissionMode::HighRiskWrite => "high_risk_write",
                }
                .into(),
                mcp_allowed_connection_ids: self.mcp_allowed_connection_ids.clone(),
                driver_settings: self.driver_settings.clone(),
                ai_strict_egress: self.ai_strict_egress.clone(),
            },
        }
    }
    pub fn apply_shared_settings(&mut self, value: SharedSettings) -> Result<(), String> {
        let mode = match value.execution.mcp_permission_mode.as_str() {
            "read_only" => McpPermissionMode::ReadOnly,
            "safe_write" => McpPermissionMode::SafeWrite,
            "high_risk_write" => McpPermissionMode::HighRiskWrite,
            _ => return Err("Invalid MCP permission mode".into()),
        };
        let execution = value.execution.normalized();
        self.theme = ThemePreference {
            mode: value.preferences.theme.mode,
            pack_id: value.preferences.theme.pack_id,
        };
        self.language = value.preferences.language;
        self.editor_font_size = value.preferences.editor_font_size;
        self.editor_font_family = value.preferences.editor_font_family;
        self.confirm_on_delete = value.preferences.confirm_on_delete;
        self.auto_commit = value.preferences.auto_commit;
        self.confirm_dangerous_execution = value.preferences.confirm_dangerous_execution;
        self.default_page_size = value.preferences.default_page_size;
        self.auto_chart_on_query = value.preferences.auto_chart_on_query;
        self.wapp_settings = value.preferences.wapp_settings;
        self.editor_completion_quote_policy = value.preferences.editor_completion_quote_policy;
        self.editor_completion_include_table_prefix =
            value.preferences.editor_completion_include_table_prefix;
        self.enable_fk_prediction = value.preferences.enable_fk_prediction;
        self.keymap_preset = value.preferences.keymap_preset;
        self.custom_keymap = value.preferences.custom_keymap;
        self.sql_execution_strategy = value.preferences.sql_execution_strategy;
        self.sql_syntax_theme = value.preferences.sql_syntax_theme;
        self.sql_format_options = value.preferences.sql_format_options;
        self.sql_snippets = value.preferences.sql_snippets;
        self.workflow_step_result_order = value.preferences.workflow_step_result_order;
        self.limit_select_results = execution.limit_select_results;
        self.query_result_limit = execution.query_result_limit;
        self.safe_mode = execution.safe_mode;
        self.connection_pool_size = execution.connection_pool_size;
        self.mcp_disabled_tools = execution.mcp_disabled_tools;
        self.mcp_permission_mode = mode;
        self.mcp_allowed_connection_ids = execution.mcp_allowed_connection_ids;
        self.driver_settings = execution.driver_settings;
        self.ai_strict_egress = execution.ai_strict_egress;
        Ok(())
    }
}
