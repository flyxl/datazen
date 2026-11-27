//! The settings-DTO re-exports the aggregate still owed — ported from React
//! `packages/frontend/src/types/index.ts` (tasklist T211).
//!
//! React's `types/index.ts` pulls four things out of
//! `@datazen/backend-client`'s `src/settings.ts`:
//!
//! * the three renamed re-exports — `SettingsMcpPermissionMode as
//!   McpPermissionMode`, `SettingsSqlFormatOptions as SqlFormatOptions`,
//!   `SettingsSqlExecutionStrategy as SqlExecutionStrategy` (the aggregate
//!   re-exports them under the frontend names, exactly the aliasing React
//!   does);
//! * and, import-only, `UserPreferences` + `ExecutionSettings` — the two
//!   interfaces `AppSettings` **extends** ([`crate::types::app_settings`]
//!   flattens them; they are legs, not aggregate-surface names).
//!
//! Source of truth: `packages/backend-client/src/settings.ts` — the
//! transport-neutral settings whitelist; native paths, launch configs and
//! credentials stay with the host. No Rust home existed (the kernel's
//! shared-settings structs are the backend's own layer, not this mirror),
//! so this module is the frontend-facing mirror.
//!
//! Pure contracts — no IO, no state — so the module compiles for wasm32
//! exactly like its `chart_types` sibling.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::connection_types::Value;

// ── The enum/struct vocabulary the three aliases carry ─────────────────────

/// React `SettingsThemePreference.mode`'s inline union `'light' | 'dark' |
/// 'system'` — how the UI resolves its theme. React declares it inline
/// with no exported name; the mirror gives it one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ThemePreferenceMode {
    Light,
    Dark,
    System,
}

impl ThemePreferenceMode {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
            Self::System => "system",
        }
    }
}

/// React `SettingsThemePreference` — the theme the app shows. `packId` is
/// required-but-nullable (`string | null`): `None` serializes as an
/// explicit `null`, absent also accepted.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsThemePreference {
    pub mode: ThemePreferenceMode,
    pub pack_id: Option<String>,
}

/// React `SettingsMcpPermissionMode = 'read_only' | 'safe_write' |
/// 'high_risk_write'` — how much MCP tool execution is allowed without an
/// approval gate.
///
/// The aggregate re-exports this under the frontend's alias
/// `McpPermissionMode` (see `types/mod.rs`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingsMcpPermissionMode {
    ReadOnly,
    SafeWrite,
    HighRiskWrite,
}

impl SettingsMcpPermissionMode {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ReadOnly => "read_only",
            Self::SafeWrite => "safe_write",
            Self::HighRiskWrite => "high_risk_write",
        }
    }
}

/// React `SettingsSqlFormatOptions.keywordCase`'s inline union `'upper' |
/// 'lower' | 'preserve'`. React declares it inline with no exported name;
/// the mirror gives it one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SqlKeywordCase {
    Upper,
    Lower,
    Preserve,
}

impl SqlKeywordCase {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Upper => "upper",
            Self::Lower => "lower",
            Self::Preserve => "preserve",
        }
    }
}

/// React `SettingsSqlFormatOptions.indentStyle`'s inline union `'2spaces' |
/// '4spaces' | 'tab'`. React declares it inline with no exported name; the
/// mirror gives it one (`2spaces` starts with a digit, so the variant needs
/// an explicit rename).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SqlIndentStyle {
    #[serde(rename = "2spaces")]
    TwoSpaces,
    #[serde(rename = "4spaces")]
    FourSpaces,
    Tab,
}

impl SqlIndentStyle {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TwoSpaces => "2spaces",
            Self::FourSpaces => "4spaces",
            Self::Tab => "tab",
        }
    }
}

/// React `SettingsSqlFormatOptions` — the SQL formatter's options.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsSqlFormatOptions {
    pub keyword_case: SqlKeywordCase,
    pub indent_style: SqlIndentStyle,
    pub break_before_boolean_operators: bool,
    /// Blank lines between formatted queries (a small count).
    pub lines_between_queries: u32,
}

/// React `SettingsSqlExecutionStrategy = 'current_statement' |
/// 'entire_script' | 'largest_statement' | 'ask'` — what a run button
/// executes.
///
/// The aggregate re-exports this under the frontend's alias
/// `SqlExecutionStrategy` (see `types/mod.rs`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingsSqlExecutionStrategy {
    CurrentStatement,
    EntireScript,
    LargestStatement,
    Ask,
}

impl SettingsSqlExecutionStrategy {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CurrentStatement => "current_statement",
            Self::EntireScript => "entire_script",
            Self::LargestStatement => "largest_statement",
            Self::Ask => "ask",
        }
    }
}

// ── The two interfaces `AppSettings` extends ────────────────────────────────

/// React `UserPreferences.editorCompletionQuotePolicy`'s inline union
/// `'unquoted' | 'always' | 'both'` — how the SQL editor quotes identifiers
/// in completions. React declares it inline with no exported name; the
/// mirror gives it one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EditorCompletionQuotePolicy {
    Unquoted,
    Always,
    Both,
}

impl EditorCompletionQuotePolicy {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unquoted => "unquoted",
            Self::Always => "always",
            Self::Both => "both",
        }
    }
}

/// React `UserPreferences.keymapPreset`'s inline union `'default' |
/// 'dbeaver' | 'navicat'` — the editor keymap family. React declares it
/// inline with no exported name; the mirror gives it one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KeymapPreset {
    Default,
    Dbeaver,
    Navicat,
}

impl KeymapPreset {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Dbeaver => "dbeaver",
            Self::Navicat => "navicat",
        }
    }
}

/// React `UserPreferences.workflowStepResultOrder`'s inline union `'asc' |
/// 'desc'`. React declares it inline with no exported name; the mirror
/// gives it one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkflowStepResultOrder {
    Asc,
    #[default]
    Desc,
}

impl WorkflowStepResultOrder {
    /// Check the persisted spelling; unknown or malformed values are not orders.
    pub fn from_wire(value: &str) -> Option<Self> {
        match value {
            "asc" => Some(Self::Asc),
            "desc" => Some(Self::Desc),
            _ => None,
        }
    }

    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Asc => "asc",
            Self::Desc => "desc",
        }
    }
}

/// React `UserPreferences.sqlSnippets`'s inline array element `{ id,
/// prefix, descriptionKey, template }` — one editor snippet. React declares
/// it inline with no exported name; the mirror gives it one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SqlSnippet {
    pub id: String,
    pub prefix: String,
    pub description_key: String,
    pub template: String,
}

/// React `UserPreferences` — the presentation-half settings blob
/// `AppSettings` extends.
///
/// Everything the form does not require is `?:` (absent stays absent) —
/// except `sqlSyntaxTheme`, which is `?: string | null`: the shared
/// transport normalizes it to an explicit `null` (`toSharedSettings`'s
/// `?? null`), so the mirror keeps the key present with `null` for `None`.
/// `wappSettings` is the opaque workspace-app bag
/// (`Record<string, unknown>` → `BTreeMap<String, Value>`, name-keyed and
/// never order-observed — the `ai_types` rule).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserPreferences {
    pub theme: SettingsThemePreference,
    pub language: String,
    /// Editor font size (px — the settings stepper's integer steps).
    pub editor_font_size: u32,
    pub editor_font_family: String,
    pub confirm_on_delete: bool,
    pub auto_commit: bool,
    pub confirm_dangerous_execution: bool,
    pub default_page_size: u32,
    pub auto_chart_on_query: bool,
    pub wapp_settings: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub editor_completion_quote_policy: Option<EditorCompletionQuotePolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub editor_completion_include_table_prefix: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enable_fk_prediction: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keymap_preset: Option<KeymapPreset>,
    /// `Partial<Record<string, string>>` — the stored custom keymap pairs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_keymap: Option<BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sql_execution_strategy: Option<SettingsSqlExecutionStrategy>,
    /// `?: string | null` — null on the wire when set to `None`; absent
    /// also accepted.
    pub sql_syntax_theme: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sql_format_options: Option<SettingsSqlFormatOptions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sql_snippets: Option<Vec<SqlSnippet>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_step_result_order: Option<WorkflowStepResultOrder>,
}

/// React `ExecutionSettings` — the execution-half settings blob
/// `AppSettings` extends.
///
/// Every field is required (the shared transport always materializes
/// them); `driverSettings` is the opaque per-driver bag
/// (`Record<string, unknown>` → `BTreeMap<String, Value>`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionSettings {
    pub limit_select_results: bool,
    /// Row cap of a query read (the family's limits width).
    pub query_result_limit: u64,
    pub safe_mode: bool,
    pub connection_pool_size: u32,
    pub mcp_disabled_tools: Vec<String>,
    pub mcp_permission_mode: SettingsMcpPermissionMode,
    pub mcp_allowed_connection_ids: Vec<String>,
    pub mcp_allowed_databases: Vec<String>,
    pub mcp_allowed_namespaces: Vec<String>,
    pub driver_settings: BTreeMap<String, Value>,
    pub ai_strict_egress: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode<T: serde::de::DeserializeOwned>(wire: serde_json::Value) -> T {
        serde_json::from_value(wire)
            .unwrap_or_else(|error| panic!("wire decode failed: {error}"))
    }

    fn encode<T: serde::Serialize>(value: &T) -> serde_json::Value {
        serde_json::to_value(value)
            .unwrap_or_else(|error| panic!("wire encode failed: {error}"))
    }

    #[test]
    fn the_inline_setting_unions_keep_their_literals() {
        let modes = [
            (ThemePreferenceMode::Light, "light"),
            (ThemePreferenceMode::Dark, "dark"),
            (ThemePreferenceMode::System, "system"),
        ];
        for (value, literal) in modes {
            assert_eq!(value.as_str(), literal);
            let parsed: ThemePreferenceMode = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(encode(&value), serde_json::json!(literal));
        }
        let permissions = [
            (SettingsMcpPermissionMode::ReadOnly, "read_only"),
            (SettingsMcpPermissionMode::SafeWrite, "safe_write"),
            (SettingsMcpPermissionMode::HighRiskWrite, "high_risk_write"),
        ];
        for (value, literal) in permissions {
            assert_eq!(value.as_str(), literal);
            let parsed: SettingsMcpPermissionMode = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(encode(&value), serde_json::json!(literal));
        }
        let strategies = [
            (SettingsSqlExecutionStrategy::CurrentStatement, "current_statement"),
            (SettingsSqlExecutionStrategy::EntireScript, "entire_script"),
            (SettingsSqlExecutionStrategy::LargestStatement, "largest_statement"),
            (SettingsSqlExecutionStrategy::Ask, "ask"),
        ];
        for (value, literal) in strategies {
            assert_eq!(value.as_str(), literal);
            let parsed: SettingsSqlExecutionStrategy = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(encode(&value), serde_json::json!(literal));
        }
        let cases = [
            (SqlKeywordCase::Upper, "upper"),
            (SqlKeywordCase::Lower, "lower"),
            (SqlKeywordCase::Preserve, "preserve"),
        ];
        for (value, literal) in cases {
            assert_eq!(value.as_str(), literal);
            let parsed: SqlKeywordCase = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(encode(&value), serde_json::json!(literal));
        }
        let indents = [
            (SqlIndentStyle::TwoSpaces, "2spaces"),
            (SqlIndentStyle::FourSpaces, "4spaces"),
            (SqlIndentStyle::Tab, "tab"),
        ];
        for (value, literal) in indents {
            assert_eq!(value.as_str(), literal);
            let parsed: SqlIndentStyle = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(encode(&value), serde_json::json!(literal));
        }
        let quotes = [
            (EditorCompletionQuotePolicy::Unquoted, "unquoted"),
            (EditorCompletionQuotePolicy::Always, "always"),
            (EditorCompletionQuotePolicy::Both, "both"),
        ];
        for (value, literal) in quotes {
            assert_eq!(value.as_str(), literal);
            let parsed: EditorCompletionQuotePolicy = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(encode(&value), serde_json::json!(literal));
        }
        let presets = [
            (KeymapPreset::Default, "default"),
            (KeymapPreset::Dbeaver, "dbeaver"),
            (KeymapPreset::Navicat, "navicat"),
        ];
        for (value, literal) in presets {
            assert_eq!(value.as_str(), literal);
            let parsed: KeymapPreset = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(encode(&value), serde_json::json!(literal));
        }
        let orders = [
            (WorkflowStepResultOrder::Asc, "asc"),
            (WorkflowStepResultOrder::Desc, "desc"),
        ];
        for (value, literal) in orders {
            assert_eq!(value.as_str(), literal);
            let parsed: WorkflowStepResultOrder = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(encode(&value), serde_json::json!(literal));
        }
    }

    #[test]
    fn user_preferences_mix_the_two_optionality_families() {
        let preferences = UserPreferences {
            theme: SettingsThemePreference {
                mode: ThemePreferenceMode::System,
                pack_id: None,
            },
            language: "zh-CN".into(),
            editor_font_size: 14,
            editor_font_family: "JetBrains Mono".into(),
            confirm_on_delete: true,
            auto_commit: false,
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
        };
        let wire = encode(&preferences);
        // `packId` and `sqlSyntaxTheme` keep explicit nulls…
        assert_eq!(wire["theme"]["packId"], serde_json::Value::Null);
        assert_eq!(wire["sqlSyntaxTheme"], serde_json::Value::Null);
        // …while every plain `?:` leg stays absent.
        assert!(wire.get("keymapPreset").is_none());
        assert!(wire.get("sqlFormatOptions").is_none());
        let back: UserPreferences = decode(wire);
        assert_eq!(back, preferences);
        // The legacy payload shapes decode too (absent keys → None).
        let legacy: UserPreferences = decode(serde_json::json!({
            "theme": { "mode": "dark", "packId": "nord" },
            "language": "en",
            "editorFontSize": 13,
            "editorFontFamily": "Menlo",
            "confirmOnDelete": false,
            "autoCommit": true,
            "confirmDangerousExecution": false,
            "defaultPageSize": 100,
            "autoChartOnQuery": true,
            "wappSettings": { "notes": { "open": true } },
            "sqlExecutionStrategy": "ask",
            "sqlSnippets": [
                { "id": "s1", "prefix": "sf", "descriptionKey": "snippet.selectFrom", "template": "SELECT * FROM " }
            ],
        }));
        assert_eq!(legacy.theme.pack_id.as_deref(), Some("nord"));
        assert_eq!(
            legacy.sql_execution_strategy,
            Some(SettingsSqlExecutionStrategy::Ask)
        );
        assert_eq!(legacy.sql_snippets.as_ref().map(|s| s.len()), Some(1));
        assert_eq!(legacy.sql_syntax_theme, None);
    }

    #[test]
    fn execution_settings_pin_their_required_wire() {
        let execution = ExecutionSettings {
            limit_select_results: true,
            query_result_limit: 1_000,
            safe_mode: false,
            connection_pool_size: 4,
            mcp_disabled_tools: vec!["shell".into()],
            mcp_permission_mode: SettingsMcpPermissionMode::SafeWrite,
            mcp_allowed_connection_ids: vec!["c1".into()],
            mcp_allowed_databases: Vec::new(),
            mcp_allowed_namespaces: Vec::new(),
            driver_settings: BTreeMap::new(),
            ai_strict_egress: true,
        };
        let wire = encode(&execution);
        assert_eq!(wire["queryResultLimit"], 1_000);
        assert_eq!(wire["mcpPermissionMode"], "safe_write");
        let back: ExecutionSettings = decode(wire);
        assert_eq!(back, execution);
    }
}
