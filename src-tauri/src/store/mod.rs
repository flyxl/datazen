//! Local encrypted persistence for connections, settings, and history.

mod ai_config;
mod connections;
mod history;
pub(crate) mod history_db;
mod key_store;
mod models;
mod settings;
mod sync_tasks;

pub use history_db::{HistoryDb, HistoryEntry, HistoryListItem, HistoryScope};
pub use models::{FavoriteQuery, QueryHistoryEntry, SyncTask};
pub use settings::{clamp_connection_pool_size, AppSettings};

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tauri::Manager;
use thiserror::Error;
use tokio::sync::{Mutex, RwLock};

use models::StoreCache;
/// Bundle identifier — must match `tauri.conf.json` `"identifier"`.
pub const APP_IDENTIFIER: &str = "com.tbeasy.datazen";

/// Monotonic counter so temp file names stay unique within this process,
/// even for back-to-back writes to the same target file.
static TMP_FILE_SEQ: AtomicU64 = AtomicU64::new(0);

/// Encrypted JSON store rooted at the per-app data directory.
pub struct Store {
    pub(super) data_dir: PathBuf,
    pub(super) encryption_key: [u8; 32],
    pub(super) cache: Arc<RwLock<StoreCache>>,
    /// Serializes disk writes so concurrent saves to the same file cannot
    /// clobber each other's temp files or leave stale snapshots on disk.
    pub(super) write_lock: Mutex<()>,
    history_db: Arc<HistoryDb>,
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("Initialization error: {0}")]
    InitError(String),

    #[error("Read error: {0}")]
    ReadError(String),

    #[error("Write error: {0}")]
    WriteError(String),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Encryption error: {0}")]
    EncryptionError(String),
}
impl Store {
    pub fn data_dir(&self) -> &PathBuf {
        &self.data_dir
    }

    pub fn history_db(&self) -> Arc<HistoryDb> {
        self.history_db.clone()
    }

    /// Default app data directory for headless entry points (MCP stdio, early logging).
    /// Matches Tauri `app_data_dir()` for the configured bundle identifier.
    pub fn default_app_data_dir() -> Result<PathBuf, StoreError> {
        dirs::data_dir()
            .map(|d| d.join(APP_IDENTIFIER))
            .ok_or_else(|| StoreError::InitError("Cannot determine data dir".into()))
    }

    pub async fn init(app_handle: &tauri::AppHandle) -> Result<Self, StoreError> {
        let data_dir = app_handle
            .path()
            .app_data_dir()
            .map_err(|e| StoreError::InitError(e.to_string()))?;

        tokio::fs::create_dir_all(&data_dir)
            .await
            .map_err(|e| StoreError::InitError(e.to_string()))?;

        let encryption_key = Self::get_or_create_encryption_key(&data_dir).await?;

        let history_db = HistoryDb::open(&data_dir).map_err(|e| StoreError::InitError(e.to_string()))?;

        let store = Self {
            data_dir,
            encryption_key,
            cache: Arc::new(RwLock::new(StoreCache::default())),
            write_lock: Mutex::new(()),
            history_db,
        };

        store.load_all().await?;
        Ok(store)
    }

    pub async fn init_with_path(data_dir: &std::path::Path) -> Result<Self, StoreError> {
        tokio::fs::create_dir_all(data_dir)
            .await
            .map_err(|e| StoreError::InitError(e.to_string()))?;

        let encryption_key = Self::get_or_create_encryption_key(data_dir).await?;

        let history_db = HistoryDb::open(data_dir).map_err(|e| StoreError::InitError(e.to_string()))?;

        let store = Self {
            data_dir: data_dir.to_path_buf(),
            encryption_key,
            cache: Arc::new(RwLock::new(StoreCache::default())),
            write_lock: Mutex::new(()),
            history_db,
        };

        store.load_all().await?;
        Ok(store)
    }

    async fn get_or_create_encryption_key(data_dir: &std::path::Path) -> Result<[u8; 32], StoreError> {
        let data_dir = data_dir.to_path_buf();
        tokio::task::spawn_blocking(move || key_store::load_or_create_master_key(&data_dir))
            .await
            .map_err(|e| StoreError::InitError(e.to_string()))?
    }

    /// Base64 encoding of the master encryption key (for user export via S1+ dialog).
    pub fn encryption_key_b64(&self) -> String {
        BASE64.encode(self.encryption_key)
    }

    #[cfg(test)]
    pub(crate) async fn get_or_create_encryption_key_for_test(
        data_dir: &std::path::Path,
    ) -> Result<[u8; 32], StoreError> {
        Self::get_or_create_encryption_key(data_dir).await
    }

    pub(super) fn encrypt(&self, plaintext: &str) -> Result<String, StoreError> {
        let key = Key::<Aes256Gcm>::from_slice(&self.encryption_key);
        let cipher = Aes256Gcm::new(key);

        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, plaintext.as_bytes())
            .map_err(|e| StoreError::EncryptionError(e.to_string()))?;

        let mut combined = nonce_bytes.to_vec();
        combined.extend(ciphertext);
        Ok(BASE64.encode(combined))
    }

    pub(super) fn decrypt(&self, encrypted: &str) -> Result<String, StoreError> {
        let combined = BASE64
            .decode(encrypted)
            .map_err(|e| StoreError::EncryptionError(e.to_string()))?;

        if combined.len() < 12 {
            return Err(StoreError::EncryptionError(
                "Invalid encrypted payload".into(),
            ));
        }

        let (nonce_bytes, ciphertext) = combined.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);
        let key = Key::<Aes256Gcm>::from_slice(&self.encryption_key);
        let cipher = Aes256Gcm::new(key);

        let plaintext = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| StoreError::EncryptionError(e.to_string()))?;

        String::from_utf8(plaintext).map_err(|e| StoreError::EncryptionError(e.to_string()))
    }

    async fn load_all(&self) -> Result<(), StoreError> {
        let mut cache = self.cache.write().await;

        cache.connections = self.load_connections_from_disk().await?;

        // First launch: store is empty, nothing to seed.
        // Users create connections via the UI.

        cache.groups = self
            .load_json_file::<Vec<String>>("groups.json")
            .await
            .unwrap_or_default();
        cache.settings = match self.load_json_file::<AppSettings>("settings.json").await {
            Ok(settings) => settings,
            Err(_) => AppSettings::default_for_first_run(),
        };

        // favorites / sync_tasks / ai_config stay unloaded
        // until their respective flows call ensure_* below.

        Ok(())
    }

    pub(super) async fn load_json_file<T>(&self, filename: &str) -> Result<T, StoreError>
    where
        T: for<'de> Deserialize<'de>,
    {
        let path = self.data_dir.join(filename);
        if !path.exists() {
            return Err(StoreError::ReadError("missing".into()));
        }

        let content = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| StoreError::ReadError(e.to_string()))?;

        serde_json::from_str(&content).map_err(|e| StoreError::ParseError(e.to_string()))
    }

    pub(super) async fn write_file_atomic(path: &std::path::Path, content: impl AsRef<[u8]>) -> Result<(), StoreError> {
        let parent = path
            .parent()
            .ok_or_else(|| StoreError::WriteError("path has no parent directory".into()))?;
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| StoreError::WriteError(e.to_string()))?;

        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| StoreError::WriteError("invalid file name".into()))?;
        let tmp_path = Self::unique_tmp_path(parent, file_name);

        tokio::fs::write(&tmp_path, content.as_ref())
            .await
            .map_err(|e| StoreError::WriteError(e.to_string()))?;
        tokio::fs::rename(&tmp_path, path)
            .await
            .map_err(|e| StoreError::WriteError(e.to_string()))?;

        Ok(())
    }

    /// A unique temp path for an atomic write. Concurrent writers (including
    /// separate app processes sharing the data dir) each get their own file,
    /// so one writer's `rename` can never lose another writer's temp file.
    fn unique_tmp_path(parent: &std::path::Path, file_name: &str) -> PathBuf {
        let seq = TMP_FILE_SEQ.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        parent.join(format!(
            ".{file_name}.{}.{}.{}.tmp",
            std::process::id(),
            nanos,
            seq
        ))
    }

    pub(super) async fn save_json_file<T: Serialize + ?Sized>(
        &self,
        filename: &str,
        data: &T,
    ) -> Result<(), StoreError> {
        let _guard = self.write_lock.lock().await;
        let path = self.data_dir.join(filename);

        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| StoreError::WriteError(e.to_string()))?;
        }

        let content =
            serde_json::to_string_pretty(data).map_err(|e| StoreError::ParseError(e.to_string()))?;

        Self::write_file_atomic(&path, content).await
    }

    pub async fn get_settings(&self) -> AppSettings {
        let cache = self.cache.read().await;
        cache.settings.clone()
    }

    pub async fn save_settings(&self, settings: AppSettings) -> Result<(), StoreError> {
        {
            let mut cache = self.cache.write().await;
            cache.settings = settings;
        }

        let snapshot = {
            let cache = self.cache.read().await;
            cache.settings.clone()
        };

        self.save_json_file("settings.json", &snapshot).await
    }
}

#[cfg(test)]
mod tests;
