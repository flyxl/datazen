//! Desktop infrastructure adapter for the module-owned settings repository.
//!
//! The adapter keeps `AppSettings`, encrypted storage, and atomic persistence in
//! the Desktop host. `datazen-modules` owns the repository boundary and all
//! shape-neutral validation; this file only translates host storage errors.

use std::sync::Arc;

use async_trait::async_trait;
use datazen_modules::settings::{SettingsRepository, SettingsRepositoryError, SharedSettings};

use crate::store::DesktopRepository;

#[derive(Clone)]
pub struct DesktopSettingsRepository {
    store: Arc<DesktopRepository>,
}

impl DesktopSettingsRepository {
    pub fn new(store: Arc<DesktopRepository>) -> Self {
        Self { store }
    }

    fn backend_error(error: impl std::fmt::Display) -> SettingsRepositoryError {
        SettingsRepositoryError::BackendUnavailable(error.to_string())
    }
}

#[async_trait]
impl SettingsRepository for DesktopSettingsRepository {
    async fn read_settings(&self) -> Result<SharedSettings, SettingsRepositoryError> {
        self.store
            .read_shared_settings()
            .await
            .map_err(Self::backend_error)
    }

    async fn write_settings(
        &self,
        settings: SharedSettings,
    ) -> Result<SharedSettings, SettingsRepositoryError> {
        self.store
            .write_shared_settings(settings)
            .await
            .map_err(Self::backend_error)
    }

    async fn read_app_settings(&self) -> Result<serde_json::Value, SettingsRepositoryError> {
        self.store
            .read_app_settings_value()
            .await
            .map_err(Self::backend_error)
    }

    async fn write_app_settings(
        &self,
        settings: serde_json::Value,
    ) -> Result<serde_json::Value, SettingsRepositoryError> {
        let mut settings: crate::store::AppSettings =
            serde_json::from_value(settings).map_err(Self::backend_error)?;
        let normalized =
            datazen_modules::settings::SettingsService::normalize(settings.shared_settings())
                .map_err(Self::backend_error)?;
        settings
            .apply_shared_settings(normalized)
            .map_err(Self::backend_error)?;
        self.store
            .write_app_settings_value(serde_json::to_value(settings).map_err(Self::backend_error)?)
            .await
            .map_err(Self::backend_error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::KeyBackend;

    #[tokio::test]
    async fn adapter_round_trips_module_settings_and_full_document() {
        let temp = tempfile::tempdir().expect("temporary settings directory");
        let store = Arc::new(
            DesktopRepository::init_with_path(temp.path(), KeyBackend::File)
                .await
                .expect("desktop repository should initialize"),
        );
        let adapter = DesktopSettingsRepository::new(store.clone());
        let service = datazen_modules::settings::SettingsService::new(std::sync::Arc::new(adapter));

        let mut shared = service.read().await.expect("module read should succeed");
        shared.preferences.language = "zh-CN".into();
        let saved = service
            .save(shared.clone())
            .await
            .expect("module save should succeed");
        assert_eq!(saved, shared);
        assert_eq!(store.get_settings().await.language, "zh-CN");

        let full = service
            .read_app_settings()
            .await
            .expect("full settings read should succeed");
        assert_eq!(full["language"], "zh-CN");
        let mut updated = full;
        updated["language"] = serde_json::json!("en-US");
        let stored = service
            .save_app_settings(updated)
            .await
            .expect("full settings save should succeed");
        assert_eq!(stored["language"], "en-US");
    }
}
