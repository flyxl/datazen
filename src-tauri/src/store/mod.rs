//! Local encrypted persistence for connections, settings, and history.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use chrono::{DateTime, Utc};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::Manager;
use thiserror::Error;
use tokio::sync::RwLock;

use crate::db::ConnectionConfig;

/// Application settings persisted on disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub theme: String,
    pub language: String,
    #[serde(default = "default_limit_select")]
    pub limit_select_results: bool,
    pub query_result_limit: u32,
    pub editor_font_size: u32,
    pub editor_font_family: String,
    pub confirm_on_delete: bool,
    pub auto_commit: bool,
    pub default_page_size: u32,
}

fn default_limit_select() -> bool {
    true
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: "dark".to_string(),
            language: "zh-CN".to_string(),
            limit_select_results: true,
            query_result_limit: 5000,
            editor_font_size: 13,
            editor_font_family: "JetBrains Mono".to_string(),
            confirm_on_delete: true,
            auto_commit: true,
            default_page_size: 50,
        }
    }
}

/// Record of a executed SQL statement.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryHistoryEntry {
    pub id: String,
    pub connection_id: String,
    pub database: String,
    pub sql: String,
    pub executed_at: DateTime<Utc>,
    pub execution_time_ms: u64,
    pub rows_affected: Option<u64>,
    pub success: bool,
    pub error_message: Option<String>,
}

#[derive(Default)]
struct StoreCache {
    connections: Vec<ConnectionConfig>,
    groups: Vec<String>,
    settings: AppSettings,
    query_history: Vec<QueryHistoryEntry>,
}

/// Encrypted JSON store rooted at the per-app data directory.
pub struct Store {
    data_dir: PathBuf,
    encryption_key: [u8; 32],
    cache: Arc<RwLock<StoreCache>>,
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
    pub async fn init(app_handle: &tauri::AppHandle) -> Result<Self, StoreError> {
        let data_dir = app_handle
            .path()
            .app_data_dir()
            .map_err(|e| StoreError::InitError(e.to_string()))?;

        tokio::fs::create_dir_all(&data_dir)
            .await
            .map_err(|e| StoreError::InitError(e.to_string()))?;

        let encryption_key = Self::get_or_create_encryption_key(&data_dir).await?;

        let store = Self {
            data_dir,
            encryption_key,
            cache: Arc::new(RwLock::new(StoreCache::default())),
        };

        store.load_all().await?;
        Ok(store)
    }

    async fn get_or_create_encryption_key(data_dir: &std::path::Path) -> Result<[u8; 32], StoreError> {
        let key_path = data_dir.join(".key");

        if let Ok(key_b64) = tokio::fs::read_to_string(&key_path).await {
            let key_bytes = BASE64
                .decode(key_b64.trim())
                .map_err(|e| StoreError::EncryptionError(e.to_string()))?;
            if key_bytes.len() != 32 {
                return Err(StoreError::EncryptionError("Invalid key length".into()));
            }
            let mut key = [0u8; 32];
            key.copy_from_slice(&key_bytes);
            return Ok(key);
        }

        let mut key = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut key);
        let key_b64 = BASE64.encode(key);
        tokio::fs::write(&key_path, key_b64.as_bytes())
            .await
            .map_err(|e| StoreError::EncryptionError(e.to_string()))?;
        Ok(key)
    }

    fn encrypt(&self, plaintext: &str) -> Result<String, StoreError> {
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

    fn decrypt(&self, encrypted: &str) -> Result<String, StoreError> {
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

        // Seed a demo connection when the store is empty (first launch).
        if cache.connections.is_empty() {
            cache.connections.push(ConnectionConfig {
                id: uuid::Uuid::new_v4().to_string(),
                name: "本地 PostgreSQL".to_string(),
                database_type: crate::db::DatabaseType::PostgreSQL,
                host: Some("localhost".to_string()),
                port: Some(5432),
                database: Some("postgres".to_string()),
                username: Some("postgres".to_string()),
                password: Some("REDACTED_PG_PASSWORD".to_string()),
                ssl_mode: crate::db::SslMode::Disable,
                connection_timeout: 30,
                ssh_tunnel: None,
                color_tag: Some("#3b82f6".to_string()),
                group: Some("开发环境".to_string()),
                last_connected_at: None,
            });
            let snapshot = cache.connections.clone();
            drop(cache);
            self.persist_connections(&snapshot).await?;
            // Re-acquire the write guard for the rest of init.
            cache = self.cache.write().await;
        }

        cache.groups = self
            .load_json_file::<Vec<String>>("groups.json")
            .await
            .unwrap_or_default();
        cache.settings = self
            .load_json_file::<AppSettings>("settings.json")
            .await
            .unwrap_or_default();
        cache.query_history = self
            .load_json_file::<Vec<QueryHistoryEntry>>("history/queries.json")
            .await
            .unwrap_or_default();

        Ok(())
    }

    async fn load_connections_from_disk(&self) -> Result<Vec<ConnectionConfig>, StoreError> {
        let path = self.data_dir.join("connections.json");
        if !path.exists() {
            return Ok(Vec::new());
        }

        let content = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| StoreError::ReadError(e.to_string()))?;

        let mut connections: Vec<ConnectionConfig> =
            serde_json::from_str(&content).map_err(|e| StoreError::ParseError(e.to_string()))?;

        for conn in &mut connections {
            if let Some(enc) = &conn.password {
                match self.decrypt(enc) {
                    Ok(plain) => conn.password = Some(plain),
                    Err(e) => {
                        tracing::warn!(conn_name = %conn.name, error = %e, "Failed to decrypt password, clearing");
                        conn.password = None;
                    }
                }
            }
        }

        Ok(connections)
    }

    async fn load_json_file<T>(&self, filename: &str) -> Result<T, StoreError>
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

    async fn save_json_file<T: Serialize + ?Sized>(
        &self,
        filename: &str,
        data: &T,
    ) -> Result<(), StoreError> {
        let path = self.data_dir.join(filename);

        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| StoreError::WriteError(e.to_string()))?;
        }

        let content =
            serde_json::to_string_pretty(data).map_err(|e| StoreError::ParseError(e.to_string()))?;

        tokio::fs::write(&path, content)
            .await
            .map_err(|e| StoreError::WriteError(e.to_string()))?;

        Ok(())
    }

    async fn persist_connections(&self, connections: &[ConnectionConfig]) -> Result<(), StoreError> {
        let mut to_disk = Vec::with_capacity(connections.len());

        for conn in connections {
            let mut c = conn.clone();
            if let Some(pw) = &c.password {
                c.password = Some(self.encrypt(pw)?);
            }
            to_disk.push(c);
        }

        self.save_json_file("connections.json", &to_disk).await
    }

    pub async fn get_connections(&self) -> Vec<ConnectionConfig> {
        let cache = self.cache.read().await;
        cache.connections.clone()
    }

    pub async fn get_connection(&self, id: &str) -> Option<ConnectionConfig> {
        let cache = self.cache.read().await;
        cache.connections.iter().find(|c| c.id == id).cloned()
    }

    pub async fn save_connection(&self, config: ConnectionConfig) -> Result<(), StoreError> {
        {
            let mut cache = self.cache.write().await;
            if let Some(pos) = cache.connections.iter().position(|c| c.id == config.id) {
                cache.connections[pos] = config.clone();
            } else {
                cache.connections.push(config.clone());
            }
        }

        let snapshot = {
            let cache = self.cache.read().await;
            cache.connections.clone()
        };

        self.persist_connections(&snapshot).await
    }

    pub async fn delete_connection(&self, id: &str) -> Result<(), StoreError> {
        {
            let mut cache = self.cache.write().await;
            cache.connections.retain(|c| c.id != id);
        }

        let snapshot = {
            let cache = self.cache.read().await;
            cache.connections.clone()
        };

        self.persist_connections(&snapshot).await
    }

    /// Returns the union of persisted custom groups and groups found on connections.
    pub async fn get_groups(&self) -> Vec<String> {
        let cache = self.cache.read().await;
        let mut set = std::collections::BTreeSet::new();
        for g in &cache.groups {
            set.insert(g.clone());
        }
        for c in &cache.connections {
            if let Some(g) = &c.group {
                set.insert(g.clone());
            }
        }
        set.into_iter().collect()
    }

    pub async fn save_groups(&self, groups: Vec<String>) -> Result<(), StoreError> {
        {
            let mut cache = self.cache.write().await;
            cache.groups = groups;
        }
        let snapshot = {
            let cache = self.cache.read().await;
            cache.groups.clone()
        };
        self.save_json_file("groups.json", &snapshot).await
    }

    pub fn decrypt_password(&self, encrypted: &str) -> Result<String, StoreError> {
        self.decrypt(encrypted)
    }

    pub async fn add_query_history(&self, entry: QueryHistoryEntry) -> Result<(), StoreError> {
        {
            let mut cache = self.cache.write().await;
            cache.query_history.insert(0, entry);
            if cache.query_history.len() > 1000 {
                cache.query_history.truncate(1000);
            }
        }

        let snapshot = {
            let cache = self.cache.read().await;
            cache.query_history.clone()
        };

        self.save_json_file("history/queries.json", &snapshot).await
    }

    pub async fn get_query_history(&self, limit: usize) -> Vec<QueryHistoryEntry> {
        let cache = self.cache.read().await;
        cache.query_history.iter().take(limit).cloned().collect()
    }

    pub async fn clear_query_history(&self) -> Result<(), StoreError> {
        {
            let mut cache = self.cache.write().await;
            cache.query_history.clear();
        }
        self.save_json_file("history/queries.json", &Vec::<QueryHistoryEntry>::new())
            .await
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

pub type ConfigStore = Store;
