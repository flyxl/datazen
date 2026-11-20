//! The `AppSettings` the aggregate declares — ported from React
//! `packages/frontend/src/types/index.ts` (tasklist T211).
//!
//! React's `types/index.ts` declares `AppSettings extends UserPreferences,
//! ExecutionSettings` — the type the app actually uses (the `./settings`
//! file's own header says so). The two legs are
//! [`crate::types::client_settings`]' mirrors; this module flattens them
//! onto `AppSettings` the way TS interface extension flattens fields —
//! `serde(flatten)`, the wire's own semantics — so the persisted blob is
//! one flat object with every preference, execution and app field at the
//! top level.
//!
//! The field set after the flatten:
//!
//! * the two legs' fields — see [`UserPreferences`] and
//!   [`ExecutionSettings`] (their names, widths and optionality rules are
//!   theirs; this module adds nothing to them). TS note: `AppSettings`
//!   redeclares `sqlSyntaxTheme?: string` — a **type-level narrowing** of
//!   the base's `?: string | null` union, not a second runtime field; the
//!   mirror keeps the single field in the flattened leg with the base's
//!   wire optionality (null on the wire for `None`), exactly what
//!   `toSharedSettings` materializes;
//! * this file's own declarations, in the React file's order:
//!   `logLevel`, `logPath`, the MCP server/HTTP trio, `contextDir`,
//!   `favoritesRoot`, `checkForUpdatesOnStartup`, `monitor`, the MCP
//!   client list and the two AI tool-id lists, `onboarding`, the three
//!   table-read settings and the DB-17 grid quartet, and
//!   `valuePresentation`.
//!
//! The leg types this module borrows: `MonitorSettings` is
//! `dashboard_types`' mirror (T159 — `types/dashboard.ts`'s own, imported
//! by React's index exactly the way this port does); `McpServerConfig`
//! is `ai_store_types`' mirror (T126); `RowCountMode`/`DefaultTableSort`/
//! `GridDensity` are [`crate::types::table_settings`]' mirrors;
//! `StoredValuePresentationSettings` is
//! [`crate::types::value_presentation`]'s. `onboarding`'s inline `{
//! completed, version }` gets a Rust name — [`OnboardingState`] — matching
//! backend-client's `SettingsOnboardingState` twin.
//!
//! Pure contract — persisted blob, no IO — so the module compiles for
//! wasm32 exactly like its `chart_types` sibling.

use serde::{Deserialize, Serialize};

use crate::ai_store_types::McpServerConfig;
use crate::dashboard_types::MonitorSettings;
use crate::types::client_settings::{ExecutionSettings, UserPreferences};
use crate::types::table_settings::{DefaultTableSort, GridDensity, RowCountMode};
use crate::types::value_presentation::StoredValuePresentationSettings;

/// React `AppSettings.logLevel`'s inline union `'trace' | 'debug' | 'info' |
/// 'warn' | 'error'` — the log verbosity the app persists. React declares
/// it inline with no exported name; the mirror gives it one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Trace => "trace",
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }
}

/// React `AppSettings.onboarding`'s inline `{ completed: boolean; version:
/// number }` — the onboarding wizard state (form-split F4's behavior
/// field, the backend-client `SettingsOnboardingState` twin: `undefined`
/// or `version < 1` → the wizard shows; a fresh install materializes
/// `completed: false`, an upgrade `completed: true`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OnboardingState {
    pub completed: bool,
    pub version: u32,
}

/// React `AppSettings` — the type the app actually uses: the persisted
/// settings blob, one flat object.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    /// The `UserPreferences` leg, flattened onto the top level exactly the
    /// way TS interface extension flattens fields.
    #[serde(flatten)]
    pub preferences: UserPreferences,
    /// The `ExecutionSettings` leg, flattened the same way.
    #[serde(flatten)]
    pub execution: ExecutionSettings,
    /// `sqlSyntaxTheme` lives in the flattened leg (see the module docs'
    /// narrowing note) — it is not redeclared here.
    pub log_level: LogLevel,
    pub log_path: String,
    /// When true, GUI may start embedded MCP on launch. Default false.
    pub mcp_server_enabled: bool,
    /// When true, the opt-in MCP Streamable HTTP listener starts on
    /// launch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp_http_enabled: Option<bool>,
    /// Configured HTTP port; 0 asks the OS for a random port.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp_http_port: Option<u16>,
    pub context_dir: String,
    /// Directory holding the favorites, one ULID-named `.sql` file each.
    /// Absent or empty = `{appData}/favorites`, which is what
    /// `getFavoritesRoot` reports. Point it at a synced folder to move
    /// favorites through iCloud/Dropbox/git.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub favorites_root: Option<String>,
    /// Check GitHub for app updates on startup (Basic builds only).
    /// Default false.
    pub check_for_updates_on_startup: bool,
    /// Dashboard monitor / tray / retention settings.
    pub monitor: MonitorSettings,
    /// Saved external MCP Client server configs. Runtime connections are
    /// separate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp_client_servers: Option<Vec<McpServerConfig>>,
    /// Plugin ids whose manifest-declared AI tools are exposed to the
    /// Agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai_plugin_tool_ids: Option<Vec<String>>,
    /// Workflow ids exposed as `workflow_<id>` Agent tools.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai_workflow_tool_ids: Option<Vec<String>>,
    /// Onboarding wizard state. `undefined` or `version < 1` → show
    /// wizard.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub onboarding: Option<OnboardingState>,
    /// How a table read obtains its row count. `auto` leaves the decision
    /// to the driver, which is what every read did before this setting
    /// existed.
    pub row_count_mode: RowCountMode,
    /// The row count above which the `auto` mode asks for an estimate
    /// instead of counting. Only the `auto` mode reads it; `<= 0` means
    /// "always count".
    pub row_count_estimate_threshold: u64,
    /// The order a table read uses when the user did not ask for one.
    /// `primaryKeyAsc` is the driver's own default, which is what every
    /// read did before this setting existed.
    pub default_table_sort: DefaultTableSort,
    /// The row density of the data grids (DB-17): a **delta** each grid
    /// applies to its own default row height. `standard` is a delta of
    /// zero — the behaviour of every grid before this setting existed,
    /// and the default.
    pub grid_density: GridDensity,
    /// Alternating row backgrounds in the data grids (DB-17). Default
    /// `true`: odd rows have been tinted since the grids exist — off is
    /// the opt-in.
    pub grid_alternating_rows: bool,
    /// The row-number gutter of the data grids (DB-17). Default `true`: the
    /// `#` column has always been there — off is the opt-in, and it trades
    /// the gutter's click-to-select affordance for one more column of
    /// data.
    pub grid_row_numbers: bool,
    /// Auto-hide grid scrollbars (DB-17): visible only while the grid is
    /// hovered. Default `false` — scrollbars stay visible, as they always
    /// were.
    pub grid_auto_hide_scrollbar: bool,
    /// Value-presentation settings (issue-32 T06, design §5/§6): versioned,
    /// namespaced per renderer id. Default is version 1 with no renderers
    /// (everything off).
    pub value_presentation: StoredValuePresentationSettings,
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::ai_store_types::McpServerTransport;
    use crate::types::client_settings::{
        EditorCompletionQuotePolicy, SettingsMcpPermissionMode, SettingsThemePreference,
        ThemePreferenceMode,
    };

    fn decode<T: serde::de::DeserializeOwned>(wire: serde_json::Value) -> T {
        serde_json::from_value(wire)
            .unwrap_or_else(|error| panic!("wire decode failed: {error}"))
    }

    fn encode<T: serde::Serialize>(value: &T) -> serde_json::Value {
        serde_json::to_value(value)
            .unwrap_or_else(|error| panic!("wire encode failed: {error}"))
    }

    fn minimal_settings() -> AppSettings {
        AppSettings {
            preferences: UserPreferences {
                theme: SettingsThemePreference {
                    mode: ThemePreferenceMode::System,
                    pack_id: None,
                },
                language: "en".into(),
                editor_font_size: 13,
                editor_font_family: "Menlo".into(),
                confirm_on_delete: true,
                auto_commit: true,
                confirm_dangerous_execution: true,
                default_page_size: 50,
                auto_chart_on_query: false,
                wapp_settings: BTreeMap::new(),
                editor_completion_quote_policy: None,
                editor_completion_include_table_prefix: None,
                enable_fk_prediction: None,
                keymap_preset: None,
                custom_keymap: None,
                sql_execution_strategy: None,
                sql_syntax_theme: None,
                sql_format_options: None,
                sql_snippets: None,
                workflow_step_result_order: None,
            },
            execution: ExecutionSettings {
                limit_select_results: false,
                query_result_limit: 1_000,
                safe_mode: false,
                connection_pool_size: 5,
                mcp_disabled_tools: Vec::new(),
                mcp_permission_mode: SettingsMcpPermissionMode::ReadOnly,
                mcp_allowed_connection_ids: Vec::new(),
                mcp_allowed_databases: Vec::new(),
                mcp_allowed_namespaces: Vec::new(),
                driver_settings: BTreeMap::new(),
                ai_strict_egress: true,
            },
            log_level: LogLevel::Info,
            log_path: "/tmp/datazen.log".into(),
            mcp_server_enabled: false,
            mcp_http_enabled: None,
            mcp_http_port: None,
            context_dir: "/tmp/context".into(),
            favorites_root: None,
            check_for_updates_on_startup: false,
            monitor: MonitorSettings {
                tray_enabled: false,
                close_to_tray: false,
                default_webhook_url: None,
                email: None,
                max_concurrent_queries: 2,
                export_include_dashboard_runs: false,
                run_retention_count: 200,
                run_retention_days: 30,
            },
            mcp_client_servers: None,
            ai_plugin_tool_ids: None,
            ai_workflow_tool_ids: None,
            onboarding: None,
            row_count_mode: RowCountMode::Auto,
            row_count_estimate_threshold: 200_000,
            default_table_sort: DefaultTableSort::PrimaryKeyAsc,
            grid_density: GridDensity::Standard,
            grid_alternating_rows: true,
            grid_row_numbers: true,
            grid_auto_hide_scrollbar: false,
            value_presentation: StoredValuePresentationSettings {
                version: 1,
                renderers: BTreeMap::new(),
                rules: None,
                column_overrides: None,
            },
        }
    }

    #[test]
    fn log_level_keeps_the_react_literals() {
        let table = [
            (LogLevel::Trace, "trace"),
            (LogLevel::Debug, "debug"),
            (LogLevel::Info, "info"),
            (LogLevel::Warn, "warn"),
            (LogLevel::Error, "error"),
        ];
        for (value, literal) in table {
            assert_eq!(value.as_str(), literal);
            let parsed: LogLevel = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(encode(&value), serde_json::json!(literal));
        }
    }

    #[test]
    fn the_two_legs_flatten_onto_one_top_level_blob() {
        let mut settings = minimal_settings();
        settings.preferences.editor_completion_quote_policy =
            Some(EditorCompletionQuotePolicy::Always);
        settings.mcp_http_port = Some(0);
        settings.mcp_client_servers = Some(vec![McpServerConfig {
            id: "fs".into(),
            name: "Filesystem".into(),
            transport: McpServerTransport::Stdio,
            command: Some("npx".into()),
            args: None,
            env: None,
            enabled: true,
            enabled_for_ai: Some(false),
        }]);
        settings.onboarding = Some(OnboardingState {
            completed: true,
            version: 1,
        });
        let wire = encode(&settings);
        // The legs' fields sit at the TOP level — TS interface extension's
        // flat runtime shape.
        assert_eq!(wire["language"], "en");
        assert_eq!(wire["editorCompletionQuotePolicy"], "always");
        assert_eq!(wire["mcpPermissionMode"], "read_only");
        assert_eq!(wire["queryResultLimit"], 1_000);
        // `sqlSyntaxTheme` is the ONE field both TS interfaces touch — it
        // lives once, in the flattened leg, with the base's null wire.
        assert_eq!(wire["sqlSyntaxTheme"], serde_json::Value::Null);
        // This file's own fields ride alongside.
        assert_eq!(wire["logLevel"], "info");
        assert_eq!(wire["mcpHttpPort"], 0);
        assert_eq!(wire["mcpClientServers"][0]["transport"], "stdio");
        assert_eq!(wire["onboarding"]["version"], 1);
        assert_eq!(wire["rowCountEstimateThreshold"], 200_000);
        // Plain `?:` legs stay absent.
        assert!(wire.get("favoritesRoot").is_none());
        assert!(wire.get("mcpHttpEnabled").is_none());
        let back: AppSettings = decode(wire);
        assert_eq!(back, settings);
    }

    #[test]
    fn the_db17_grid_quartet_and_row_count_trio_pin_their_wire() {
        let settings = minimal_settings();
        let wire = encode(&settings);
        assert_eq!(wire["rowCountMode"], "auto");
        assert_eq!(wire["defaultTableSort"], "primaryKeyAsc");
        assert_eq!(wire["gridDensity"], "standard");
        assert_eq!(wire["gridAlternatingRows"], true);
        assert_eq!(wire["gridRowNumbers"], true);
        assert_eq!(wire["gridAutoHideScrollbar"], false);
        // The onboarding wizard's absence stays absent (`undefined` or
        // `version < 1` → show wizard).
        assert!(wire.get("onboarding").is_none());
    }

    #[test]
    fn a_settings_blob_with_only_required_keys_decodes() {
        // The legs' required fields + this file's required fields; every
        // optional leg reads as its absence.
        let wire = serde_json::json!({
            "theme": { "mode": "light", "packId": null },
            "language": "zh-CN",
            "editorFontSize": 14,
            "editorFontFamily": "JetBrains Mono",
            "confirmOnDelete": false,
            "autoCommit": false,
            "confirmDangerousExecution": false,
            "defaultPageSize": 100,
            "autoChartOnQuery": true,
            "wappSettings": {},
            "sqlSyntaxTheme": "nord",
            "limitSelectResults": true,
            "queryResultLimit": 500,
            "safeMode": true,
            "connectionPoolSize": 8,
            "mcpDisabledTools": [],
            "mcpPermissionMode": "high_risk_write",
            "mcpAllowedConnectionIds": [],
            "mcpAllowedDatabases": [],
            "mcpAllowedNamespaces": [],
            "driverSettings": {},
            "aiStrictEgress": false,
            "logLevel": "warn",
            "logPath": "/var/log/datazen.log",
            "mcpServerEnabled": true,
            "contextDir": "~/Documents/context",
            "checkForUpdatesOnStartup": true,
            "monitor": {
                "trayEnabled": true,
                "closeToTray": true,
                "maxConcurrentQueries": 2,
                "exportIncludeDashboardRuns": true,
                "runRetentionCount": 200,
                "runRetentionDays": 30
            },
            "rowCountMode": "none",
            "rowCountEstimateThreshold": 0,
            "defaultTableSort": "none",
            "gridDensity": "compact",
            "gridAlternatingRows": false,
            "gridRowNumbers": false,
            "gridAutoHideScrollbar": true,
            "valuePresentation": { "version": 1, "renderers": {} }
        });
        let settings: AppSettings = decode(wire);
        assert_eq!(settings.log_level, LogLevel::Warn);
        // `sqlSyntaxTheme` is the one narrowed field — read it off the
        // flattened leg (`?: string` view: Some iff set).
        assert_eq!(settings.preferences.sql_syntax_theme.as_deref(), Some("nord"));
        assert_eq!(settings.row_count_mode, RowCountMode::None);
        assert_eq!(settings.grid_density, GridDensity::Compact);
        assert_eq!(settings.onboarding, None);
        assert_eq!(settings.mcp_http_port, None);
    }
}
