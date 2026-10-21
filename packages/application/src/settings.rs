//! Portable settings use cases. Native side effects are owned by the host adapter.
use datazen_platform_api::{ports::SettingsRepository, settings::SharedSettings, PortError};
#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("{0}")]
    Validation(String),
    #[error(transparent)]
    Repository(#[from] PortError),
}
pub struct SettingsService<'a> {
    repository: &'a dyn SettingsRepository,
}
impl<'a> SettingsService<'a> {
    pub fn new(repository: &'a dyn SettingsRepository) -> Self {
        Self { repository }
    }
    pub async fn read(&self) -> Result<SharedSettings, SettingsError> {
        Ok(self.repository.read_settings().await?)
    }
    pub async fn save(&self, settings: SharedSettings) -> Result<SharedSettings, SettingsError> {
        Ok(self
            .repository
            .write_settings(Self::normalize(settings)?)
            .await?)
    }
    pub fn normalize(mut settings: SharedSettings) -> Result<SharedSettings, SettingsError> {
        let p = &settings.preferences;
        let valid = matches!(p.theme.mode.as_str(), "light" | "dark" | "system")
            && matches!(
                p.editor_completion_quote_policy.as_str(),
                "unquoted" | "always" | "both"
            )
            && matches!(p.keymap_preset.as_str(), "default" | "dbeaver" | "navicat")
            && matches!(
                p.sql_execution_strategy.as_str(),
                "current_statement" | "entire_script" | "largest_statement" | "ask"
            )
            && matches!(p.workflow_step_result_order.as_str(), "asc" | "desc")
            && matches!(
                p.sql_format_options.keyword_case.as_str(),
                "upper" | "lower" | "preserve"
            )
            && matches!(
                p.sql_format_options.indent_style.as_str(),
                "2spaces" | "4spaces" | "tab"
            )
            && matches!(
                settings.execution.mcp_permission_mode.as_str(),
                "read_only" | "safe_write" | "high_risk_write"
            );
        if !valid {
            return Err(SettingsError::Validation(
                "Invalid settings enum value".into(),
            ));
        }
        settings.execution = settings.execution.normalized();
        Ok(settings)
    }
}
