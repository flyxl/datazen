//! Desktop infrastructure adapter for the module-owned settings repository.
//!
//! The adapter keeps `AppSettings`, encrypted storage, and atomic persistence in
//! the Desktop host. `datazen` owns the repository boundary and all
//! shape-neutral validation; this file only translates host storage errors.

use std::sync::Arc;

use async_trait::async_trait;
use datazen::settings::{SettingsRepository, SettingsRepositoryError, SharedSettings};

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
        let normalized = datazen::settings::SettingsService::normalize(settings.shared_settings())
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
        let service = datazen::settings::SettingsService::new(std::sync::Arc::new(adapter));

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

    /// Issue-32 T06 desktop promotion (F4-style): the loss scenario. Before
    /// the promotion, `shared_settings()` hard-coded the default, so a
    /// `value_presentation` saved through either path read back as the
    /// default — and the next save overwrote the real value with it.
    #[tokio::test]
    async fn adapter_round_trips_value_presentation_through_shared_and_full_document() {
        use datazen::settings::ValuePresentationSettings;

        let temp = tempfile::tempdir().expect("temporary settings directory");
        let store = Arc::new(
            DesktopRepository::init_with_path(temp.path(), KeyBackend::File)
                .await
                .expect("desktop repository should initialize"),
        );
        let adapter = DesktopSettingsRepository::new(store.clone());
        let service = datazen::settings::SettingsService::new(std::sync::Arc::new(adapter));

        let mut renderers = serde_json::Map::new();
        renderers.insert(
            "datazen.timestamp".into(),
            serde_json::json!({
                "enabled": true,
                "options": {
                    "interpretation": "instant",
                    "format": "iso",
                    "displayTimeZone": "UTC",
                },
            }),
        );
        let expected = ValuePresentationSettings {
            version: 1,
            renderers,
        };

        // Shared path: save via SettingsService → read returns the same value.
        let mut shared = service.read().await.expect("module read should succeed");
        shared.value_presentation = expected.clone();
        let saved = service
            .save(shared.clone())
            .await
            .expect("module save should succeed");
        assert_eq!(saved.value_presentation, expected);
        let reread = service.read().await.expect("module re-read should succeed");
        assert_eq!(
            reread.value_presentation, expected,
            "shared read must return the saved value, not the default"
        );

        // Full-document path: save_app_settings with valuePresentation in the
        // doc → the shared read presents the same value.
        let mut full = service
            .read_app_settings()
            .await
            .expect("full settings read should succeed");
        full["valuePresentation"] =
            serde_json::to_value(&expected).expect("value presentation serializes");
        let stored = service
            .save_app_settings(full)
            .await
            .expect("full settings save should succeed");
        assert_eq!(
            stored["valuePresentation"]["renderers"]["datazen.timestamp"]["enabled"],
            true
        );
        let via_shared = service.read().await.expect("module read should succeed");
        assert_eq!(
            via_shared.value_presentation, expected,
            "shared read after a full-document save must present the saved value"
        );
    }
}
