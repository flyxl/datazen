//! The host's settings file (N8): the app-settings document as one JSON
//! file under the data directory, written atomically. The UI's three
//! settings live under its `"ui"` key.
//!
//! The shared half of the repository contract (the typed `SharedSettings`
//! document) is refused, not faked: nothing in the native host reads or
//! writes it yet, and an empty-but-plausible answer would lie about a
//! document that does not exist.

use async_trait::async_trait;
use datazen::settings::domain::{
    SettingsRepository, SettingsRepositoryError, SharedSettings,
};
use std::path::{Path, PathBuf};

pub struct FileSettingsRepository {
    path: PathBuf,
}

impl FileSettingsRepository {
    pub fn open(path: &Path) -> Self {
        Self { path: path.to_owned() }
    }

    fn io_error(error: std::io::Error) -> SettingsRepositoryError {
        SettingsRepositoryError::BackendUnavailable(error.to_string())
    }

    /// The document: `{}` when the file is missing (a fresh install has no
    /// settings), the parsed file otherwise. A file that exists but does not
    /// parse is an error — never silently replaced.
    fn document(&self) -> Result<serde_json::Value, SettingsRepositoryError> {
        match std::fs::read_to_string(&self.path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(serde_json::json!({}))
            }
            Err(error) => Err(Self::io_error(error)),
            Ok(text) if text.trim().is_empty() => Ok(serde_json::json!({})),
            Ok(text) => serde_json::from_str(&text).map_err(|error| {
                SettingsRepositoryError::BackendUnavailable(format!(
                    "parse {}: {error}",
                    self.path.display()
                ))
            }),
        }
    }

    /// Write atomically: a sibling temporary file, then rename.
    fn persist(&self, document: &serde_json::Value) -> Result<(), SettingsRepositoryError> {
        let text = serde_json::to_string_pretty(document).map_err(|error| {
            SettingsRepositoryError::BackendUnavailable(format!("serialize the settings: {error}"))
        })?;
        let temp = self.path.with_extension("json.tmp");
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(Self::io_error)?;
        }
        std::fs::write(&temp, text).map_err(Self::io_error)?;
        std::fs::rename(&temp, &self.path).map_err(Self::io_error)?;
        Ok(())
    }
}

#[async_trait]
impl SettingsRepository for FileSettingsRepository {
    async fn read_settings(&self) -> Result<SharedSettings, SettingsRepositoryError> {
        // The native host has no shared-settings document yet; refusing is
        // the honest answer (the UI's settings live in the app half).
        Err(SettingsRepositoryError::BackendUnavailable(
            "the native host stores no shared settings document".into(),
        ))
    }

    async fn write_settings(
        &self,
        _settings: SharedSettings,
    ) -> Result<SharedSettings, SettingsRepositoryError> {
        Err(SettingsRepositoryError::BackendUnavailable(
            "the native host stores no shared settings document".into(),
        ))
    }

    async fn read_app_settings(&self) -> Result<serde_json::Value, SettingsRepositoryError> {
        self.document()
    }

    async fn write_app_settings(
        &self,
        settings: serde_json::Value,
    ) -> Result<serde_json::Value, SettingsRepositoryError> {
        self.persist(&settings)?;
        Ok(settings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen::settings::application::SettingsService;
    use std::sync::Arc;

    fn temp_path(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("datazen-rune-settings-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("settings.json")
    }

    #[test]
    fn a_missing_file_reads_empty_and_the_first_write_lands() {
        let path = temp_path("fresh");
        let service = SettingsService::new(Arc::new(FileSettingsRepository::open(&path)));
        let document = futures_executor::block_on(service.read_app_settings()).unwrap();
        assert!(document.get("ui").is_none(), "a fresh install has no ui settings");

        futures_executor::block_on(
            service.save_app_settings(serde_json::json!({
                "ui": { "theme": "dark", "tablePageSize": 50 }
            })),
        )
        .unwrap();
        assert!(path.exists(), "the write landed on disk");

        // A fresh repository over the same file sees it: persistence, not a
        // session cache.
        let service = SettingsService::new(Arc::new(FileSettingsRepository::open(&path)));
        let document = futures_executor::block_on(service.read_app_settings()).unwrap();
        assert_eq!(document["ui"]["tablePageSize"], 50);
    }

    #[test]
    fn a_corrupt_file_refuses_to_read() {
        let path = temp_path("corrupt");
        std::fs::write(&path, "{ not json").unwrap();
        let service = SettingsService::new(Arc::new(FileSettingsRepository::open(&path)));
        let error = futures_executor::block_on(service.read_app_settings()).unwrap_err();
        assert!(!error.to_string().is_empty(), "the failure carries a sentence");
    }
}
