//! Settings domain vocabulary and the repository boundary owned by the module.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Form-split F4: the three behavior fields the desktop blob used to own.
///
/// `monitor` / `onboarding` / `row_count_mode` are behavior, not host
/// topology: a browser form factor needs them exactly as the desktop does, so
/// they moved from the desktop-only settings file into the shared contract.
/// Paths, process and updater fields stay behind on the host (`DesktopSettings`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SharedSettings {
    pub preferences: UserPreferences,
    pub execution: ExecutionSettings,
    /// Form-split F4 behavior fields; `default` keeps older payloads loadable.
    #[serde(default)]
    pub monitor: MonitorSettings,
    #[serde(default)]
    pub onboarding: Option<OnboardingState>,
    #[serde(default = "default_row_count_mode")]
    pub row_count_mode: String,
}

fn default_row_count_mode() -> String {
    "auto".into()
}

/// Onboarding wizard state, portable across form factors.
///
/// `None` or `version < 1` → show the wizard. A fresh install materializes
/// `for_fresh_install`; an upgrade materializes `for_existing_install`
/// (upgrade ≠ first run) — the host decides which at load time.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OnboardingState {
    pub completed: bool,
    pub version: i32,
}

impl OnboardingState {
    /// Fresh installation: the journey has never been shown.
    pub fn for_fresh_install(version: i32) -> Self {
        Self {
            completed: false,
            version,
        }
    }

    /// Existing installation that predates the journey: mark it as seen so an
    /// upgrading user is never onboarded.
    pub fn for_existing_install(version: i32) -> Self {
        Self {
            completed: true,
            version,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PortableThemePreference {
    pub mode: String,
    pub pack_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct SqlFormatOptions {
    pub keyword_case: String,
    pub indent_style: String,
    pub break_before_boolean_operators: bool,
    pub lines_between_queries: u32,
}

impl Default for SqlFormatOptions {
    fn default() -> Self {
        Self {
            keyword_case: "upper".into(),
            indent_style: "2spaces".into(),
            break_before_boolean_operators: true,
            lines_between_queries: 1,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SqlSnippet {
    pub id: String,
    pub prefix: String,
    pub description_key: String,
    pub template: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UserPreferences {
    pub theme: PortableThemePreference,
    pub language: String,
    pub editor_font_size: u32,
    pub editor_font_family: String,
    pub confirm_on_delete: bool,
    pub auto_commit: bool,
    pub confirm_dangerous_execution: bool,
    pub default_page_size: u32,
    pub auto_chart_on_query: bool,
    pub wapp_settings: serde_json::Map<String, serde_json::Value>,
    pub editor_completion_quote_policy: String,
    pub editor_completion_include_table_prefix: bool,
    pub enable_fk_prediction: bool,
    pub keymap_preset: String,
    pub custom_keymap: std::collections::HashMap<String, String>,
    pub sql_execution_strategy: String,
    pub sql_syntax_theme: Option<String>,
    pub sql_format_options: SqlFormatOptions,
    pub sql_snippets: Vec<SqlSnippet>,
    pub workflow_step_result_order: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecutionSettings {
    pub limit_select_results: bool,
    pub query_result_limit: u32,
    pub safe_mode: bool,
    pub connection_pool_size: u32,
    pub mcp_disabled_tools: Vec<String>,
    pub mcp_permission_mode: String,
    pub mcp_allowed_connection_ids: Vec<String>,
    pub driver_settings: serde_json::Map<String, serde_json::Value>,
    pub ai_strict_egress: bool,
}

impl ExecutionSettings {
    pub fn result_limit(&self) -> Option<u32> {
        (self.limit_select_results && self.query_result_limit > 0)
            .then_some(self.query_result_limit)
    }

    pub fn normalized(mut self) -> Self {
        self.connection_pool_size = self.connection_pool_size.clamp(1, 100);
        self
    }

    /// Enforce the backend cap even when a client supplies a larger limit.
    pub fn apply_result_limit(&self, input: &mut serde_json::Value) {
        if let Some(cap) = self.result_limit() {
            let requested = input.get("limit").and_then(serde_json::Value::as_u64);
            input["limit"] = serde_json::json!(requested
                .filter(|n| *n > 0)
                .map(|n| n.min(u64::from(cap)))
                .unwrap_or(u64::from(cap)));
        }
    }
}

/// Infrastructure facts returned by a settings repository.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SettingsRepositoryError {
    #[error("backend unavailable: {0}")]
    BackendUnavailable(String),
}

/// Settings persistence bound by the host to its authorized scope.
#[async_trait]
pub trait SettingsRepository: Send + Sync + 'static {
    async fn read_settings(&self) -> Result<SharedSettings, SettingsRepositoryError>;
    async fn write_settings(
        &self,
        settings: SharedSettings,
    ) -> Result<SharedSettings, SettingsRepositoryError>;

    /// Read the host-owned full settings document without exposing its concrete type here.
    async fn read_app_settings(&self) -> Result<serde_json::Value, SettingsRepositoryError>;

    /// Persist the host-owned full settings document and return its stored snapshot.
    async fn write_app_settings(
        &self,
        settings: serde_json::Value,
    ) -> Result<serde_json::Value, SettingsRepositoryError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execution_settings_normalization_preserves_result_limit_semantics() {
        let settings = ExecutionSettings {
            limit_select_results: true,
            query_result_limit: 50,
            safe_mode: true,
            connection_pool_size: 0,
            mcp_disabled_tools: Vec::new(),
            mcp_permission_mode: "safe_write".into(),
            mcp_allowed_connection_ids: Vec::new(),
            driver_settings: serde_json::Map::new(),
            ai_strict_egress: true,
        };

        let normalized = settings.normalized();
        assert_eq!(normalized.connection_pool_size, 1);
        assert_eq!(normalized.result_limit(), Some(50));
    }

    #[test]
    fn result_limit_cannot_be_bypassed_by_client_limit() {
        let settings = ExecutionSettings {
            limit_select_results: true,
            query_result_limit: 100,
            safe_mode: true,
            connection_pool_size: 10,
            mcp_disabled_tools: vec![],
            mcp_permission_mode: "safe_write".into(),
            mcp_allowed_connection_ids: vec![],
            driver_settings: serde_json::Map::new(),
            ai_strict_egress: true,
        };

        for supplied in [
            serde_json::json!(10000),
            serde_json::json!(0),
            serde_json::Value::Null,
        ] {
            let mut input = serde_json::json!({ "limit": supplied });
            settings.apply_result_limit(&mut input);
            assert_eq!(input["limit"], 100);
        }
    }
}

/// SMTP settings reserved for monitor email delivery.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MonitorEmailSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<Vec<String>>,
}

/// Dashboard monitor / tray / retention settings (form-split F4: a shared
/// settings field, so it lives in the ungated settings domain; the dashboard
/// module re-exports it).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MonitorSettings {
    pub tray_enabled: bool,
    pub close_to_tray: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_webhook_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<MonitorEmailSettings>,
    pub max_concurrent_queries: u32,
    pub export_include_dashboard_runs: bool,
    pub run_retention_count: u32,
    pub run_retention_days: u32,
}

impl Default for MonitorSettings {
    fn default() -> Self {
        Self {
            tray_enabled: true,
            close_to_tray: true,
            default_webhook_url: None,
            email: None,
            max_concurrent_queries: 2,
            export_include_dashboard_runs: true,
            run_retention_count: 200,
            run_retention_days: 30,
        }
    }
}
