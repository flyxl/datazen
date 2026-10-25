//! Settings application use cases.

use super::domain::{SettingsRepository, SettingsRepositoryError, SharedSettings};

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("{0}")]
    Validation(String),
    #[error(transparent)]
    Repository(#[from] SettingsRepositoryError),
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

    /// Read the host-owned full settings document through the injected port.
    pub async fn read_app_settings(&self) -> Result<serde_json::Value, SettingsError> {
        Ok(self.repository.read_app_settings().await?)
    }

    /// Persist the host-owned full settings document through the injected port.
    pub async fn save_app_settings(
        &self,
        settings: serde_json::Value,
    ) -> Result<serde_json::Value, SettingsError> {
        Ok(self.repository.write_app_settings(settings).await?)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::domain::{
        ExecutionSettings, PortableThemePreference, SqlFormatOptions, UserPreferences,
    };
    use std::sync::Mutex;

    fn sample_settings() -> SharedSettings {
        SharedSettings {
            preferences: UserPreferences {
                theme: PortableThemePreference {
                    mode: "dark".into(),
                    pack_id: None,
                },
                language: "en-US".into(),
                editor_font_size: 14,
                editor_font_family: "monospace".into(),
                confirm_on_delete: true,
                auto_commit: true,
                confirm_dangerous_execution: true,
                default_page_size: 100,
                auto_chart_on_query: false,
                wapp_settings: serde_json::Map::new(),
                editor_completion_quote_policy: "unquoted".into(),
                editor_completion_include_table_prefix: true,
                enable_fk_prediction: false,
                keymap_preset: "default".into(),
                custom_keymap: std::collections::HashMap::new(),
                sql_execution_strategy: "current_statement".into(),
                sql_syntax_theme: None,
                sql_format_options: SqlFormatOptions::default(),
                sql_snippets: Vec::new(),
                workflow_step_result_order: "desc".into(),
            },
            execution: ExecutionSettings {
                limit_select_results: true,
                query_result_limit: 1000,
                safe_mode: true,
                connection_pool_size: 10,
                mcp_disabled_tools: Vec::new(),
                mcp_permission_mode: "safe_write".into(),
                mcp_allowed_connection_ids: Vec::new(),
                driver_settings: serde_json::Map::new(),
                ai_strict_egress: true,
            },
        }
    }

    struct RecordingRepository {
        stored: Mutex<SharedSettings>,
        fail: bool,
    }

    #[async_trait::async_trait]
    impl SettingsRepository for RecordingRepository {
        async fn read_settings(&self) -> Result<SharedSettings, SettingsRepositoryError> {
            if self.fail {
                return Err(SettingsRepositoryError::BackendUnavailable(
                    "read failed".into(),
                ));
            }
            self.stored
                .lock()
                .map(|settings| settings.clone())
                .map_err(|_| SettingsRepositoryError::BackendUnavailable("lock poisoned".into()))
        }

        async fn write_settings(
            &self,
            settings: SharedSettings,
        ) -> Result<SharedSettings, SettingsRepositoryError> {
            if self.fail {
                return Err(SettingsRepositoryError::BackendUnavailable(
                    "write failed".into(),
                ));
            }
            let mut stored = self
                .stored
                .lock()
                .map_err(|_| SettingsRepositoryError::BackendUnavailable("lock poisoned".into()))?;
            *stored = settings.clone();
            Ok(settings)
        }

        async fn read_app_settings(&self) -> Result<serde_json::Value, SettingsRepositoryError> {
            Ok(serde_json::json!({ "settings": "opaque" }))
        }

        async fn write_app_settings(
            &self,
            settings: serde_json::Value,
        ) -> Result<serde_json::Value, SettingsRepositoryError> {
            Ok(settings)
        }
    }

    #[tokio::test]
    async fn read_and_save_use_the_injected_repository() {
        let repository = RecordingRepository {
            stored: Mutex::new(sample_settings()),
            fail: false,
        };
        let service = SettingsService::new(&repository);
        assert_eq!(
            service.read().await.expect("read should succeed"),
            sample_settings()
        );

        let mut next = sample_settings();
        next.preferences.language = "zh-CN".into();
        let saved = service
            .save(next.clone())
            .await
            .expect("save should succeed");
        assert_eq!(saved, next);
        assert_eq!(service.read().await.expect("read after save"), next);
    }

    #[tokio::test]
    async fn invalid_enum_is_rejected_before_repository_write() {
        let repository = RecordingRepository {
            stored: Mutex::new(sample_settings()),
            fail: false,
        };
        let service = SettingsService::new(&repository);
        let mut invalid = sample_settings();
        invalid.preferences.theme.mode = "neon".into();

        let error = service
            .save(invalid)
            .await
            .expect_err("invalid mode must fail");
        assert!(
            matches!(error, SettingsError::Validation(message) if message == "Invalid settings enum value")
        );
        assert_eq!(
            service
                .read()
                .await
                .expect("stored settings remain readable"),
            sample_settings()
        );
    }

    #[tokio::test]
    async fn normalization_clamps_connection_pool_size() {
        let repository = RecordingRepository {
            stored: Mutex::new(sample_settings()),
            fail: false,
        };
        let service = SettingsService::new(&repository);
        let mut settings = sample_settings();
        settings.execution.connection_pool_size = 1000;

        let saved = service.save(settings).await.expect("save should normalize");
        assert_eq!(saved.execution.connection_pool_size, 100);
    }

    #[tokio::test]
    async fn repository_failure_is_preserved_as_application_error() {
        let repository = RecordingRepository {
            stored: Mutex::new(sample_settings()),
            fail: true,
        };
        let service = SettingsService::new(&repository);

        let error = service
            .read()
            .await
            .expect_err("repository failure must surface");
        assert!(matches!(
            error,
            SettingsError::Repository(SettingsRepositoryError::BackendUnavailable(message))
                if message == "read failed"
        ));

        let write_error = service
            .save(sample_settings())
            .await
            .expect_err("repository write failure must surface");
        assert!(matches!(
            write_error,
            SettingsError::Repository(SettingsRepositoryError::BackendUnavailable(message))
                if message == "write failed"
        ));
    }
}
