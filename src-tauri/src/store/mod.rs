//! Local encrypted persistence for connections, settings, and history.

mod key_store;

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

use crate::ai::AiProviderConfig;
use crate::dashboard::types::MonitorSettings;
use crate::db::ConnectionConfig;
use crate::mcp::permission::McpPermissionMode;

/// Light / dark / system mode plus optional installed theme pack.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ThemePreference {
    pub mode: String,
    #[serde(default)]
    pub pack_id: Option<String>,
}

impl Default for ThemePreference {
    fn default() -> Self {
        Self {
            mode: "dark".into(),
            pack_id: None,
        }
    }
}

fn deserialize_theme<'de, D>(deserializer: D) -> Result<ThemePreference, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::String(s) if matches!(s.as_str(), "light" | "dark" | "system") => {
            Ok(ThemePreference {
                mode: s,
                pack_id: None,
            })
        }
        other => serde_json::from_value(other).map_err(serde::de::Error::custom),
    }
}

/// Application settings persisted on disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    #[serde(deserialize_with = "deserialize_theme", default)]
    pub theme: ThemePreference,
    pub language: String,
    #[serde(default = "default_limit_select")]
    pub limit_select_results: bool,
    pub query_result_limit: u32,
    pub editor_font_size: u32,
    pub editor_font_family: String,
    pub confirm_on_delete: bool,
    pub auto_commit: bool,
    pub default_page_size: u32,
    #[serde(default = "default_log_level")]
    pub log_level: String,
    #[serde(default)]
    pub log_path: String,
    /// When true, GUI may start an embedded MCP stdio server on launch.
    /// Default false — MCP for external clients should use `datazen --mcp`.
    #[serde(default)]
    pub mcp_server_enabled: bool,
    #[serde(default)]
    pub mcp_disabled_tools: Vec<String>,
    /// MCP tool permission tier for external AI clients (default: safe_write).
    #[serde(default)]
    pub mcp_permission_mode: McpPermissionMode,
    /// Persistent connection config IDs exposed to MCP. Empty = all connections.
    #[serde(default)]
    pub mcp_allowed_connection_ids: Vec<String>,
    #[serde(default)]
    pub context_dir: String,
    /// When true, GUI checks for app updates on startup (default off).
    #[serde(default)]
    pub check_for_updates_on_startup: bool,
    /// After a successful query, switch to chart view when the result is chartable.
    #[serde(default = "default_true")]
    pub auto_chart_on_query: bool,
    /// Dashboard monitor / tray / retention settings (nested for settings UI).
    #[serde(default)]
    pub monitor: MonitorSettings,
    /// Opaque per-plugin settings keyed by plugin id (e.g. `"redis"`).
    #[serde(default)]
    pub plugin_settings: serde_json::Map<String, serde_json::Value>,
}

fn default_limit_select() -> bool {
    false
}

fn default_true() -> bool {
    true
}

fn default_log_level() -> String {
    "info".to_string()
}

impl AppSettings {
    /// Defaults used on first install when `settings.json` is absent.
    pub fn default_for_first_run() -> Self {
        let mut settings = Self::default();
        settings.language = crate::i18n_locale::default_ui_language();
        settings
    }
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: ThemePreference::default(),
            language: "en".to_string(),
            limit_select_results: false,
            query_result_limit: 5000,
            editor_font_size: 13,
            editor_font_family: "JetBrains Mono".to_string(),
            confirm_on_delete: true,
            auto_commit: true,
            default_page_size: 50,
            log_level: default_log_level(),
            log_path: String::new(),
            mcp_server_enabled: false,
            mcp_disabled_tools: Vec::new(),
            mcp_permission_mode: McpPermissionMode::default(),
            mcp_allowed_connection_ids: Vec::new(),
            context_dir: String::new(),
            check_for_updates_on_startup: false,
            auto_chart_on_query: true,
            monitor: MonitorSettings::default(),
            plugin_settings: serde_json::Map::new(),
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FavoriteQuery {
    pub id: String,
    pub title: String,
    pub sql: String,
    pub created_at: DateTime<Utc>,
}

/// Persisted state for a data-sync task (checkpoint / resume).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncTask {
    pub id: String,
    pub source_connection_id: String,
    pub target_connection_id: String,
    pub source_config_id: String,
    pub target_config_id: String,
    /// All tables selected for sync.
    pub tables: Vec<String>,
    /// Tables that have been fully synced.
    pub completed_tables: Vec<String>,
    /// Table that was being synced when interrupted (if any).
    pub current_table: Option<String>,
    /// Row offset within the current table (rows already inserted).
    pub current_table_offset: u64,
    /// Source row count snapshot at task creation, keyed by table name.
    pub source_row_counts: std::collections::HashMap<String, u64>,
    /// "full" | "continue"
    pub strategy: String,
    /// "running" | "paused" | "completed" | "failed"
    pub status: String,
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Default)]
struct StoreCache {
    connections: Vec<ConnectionConfig>,
    groups: Vec<String>,
    settings: AppSettings,
    /// Lazy: loaded on first history / favorites / sync / AI access.
    query_history: Vec<QueryHistoryEntry>,
    query_history_loaded: bool,
    favorite_queries: Vec<FavoriteQuery>,
    favorite_queries_loaded: bool,
    sync_tasks: Vec<SyncTask>,
    sync_tasks_loaded: bool,
    ai_config: Option<AiProviderConfig>,
    ai_config_loaded: bool,
}

/// Bundle identifier — must match `tauri.conf.json` `"identifier"`.
pub const APP_IDENTIFIER: &str = "com.tbeasy.datazen";

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
    pub fn data_dir(&self) -> &PathBuf {
        &self.data_dir
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

        let store = Self {
            data_dir,
            encryption_key,
            cache: Arc::new(RwLock::new(StoreCache::default())),
        };

        store.load_all().await?;
        Ok(store)
    }

    pub async fn init_with_path(data_dir: &std::path::Path) -> Result<Self, StoreError> {
        tokio::fs::create_dir_all(data_dir)
            .await
            .map_err(|e| StoreError::InitError(e.to_string()))?;

        let encryption_key = Self::get_or_create_encryption_key(data_dir).await?;

        let store = Self {
            data_dir: data_dir.to_path_buf(),
            encryption_key,
            cache: Arc::new(RwLock::new(StoreCache::default())),
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

        // query_history / favorites / sync_tasks / ai_config stay unloaded
        // until their respective flows call ensure_* below.

        Ok(())
    }

    async fn ensure_query_history_loaded(&self) {
        {
            let cache = self.cache.read().await;
            if cache.query_history_loaded {
                return;
            }
        }
        let data = self
            .load_json_file::<Vec<QueryHistoryEntry>>("history/queries.json")
            .await
            .unwrap_or_default();
        let mut cache = self.cache.write().await;
        if cache.query_history_loaded {
            return;
        }
        cache.query_history = data;
        cache.query_history_loaded = true;
        tracing::debug!(
            count = cache.query_history.len(),
            "Loaded query history on demand"
        );
    }

    async fn ensure_favorite_queries_loaded(&self) {
        {
            let cache = self.cache.read().await;
            if cache.favorite_queries_loaded {
                return;
            }
        }
        let data = self
            .load_json_file::<Vec<FavoriteQuery>>("favorites/queries.json")
            .await
            .unwrap_or_default();
        let mut cache = self.cache.write().await;
        if cache.favorite_queries_loaded {
            return;
        }
        cache.favorite_queries = data;
        cache.favorite_queries_loaded = true;
        tracing::debug!(
            count = cache.favorite_queries.len(),
            "Loaded favorite queries on demand"
        );
    }

    async fn ensure_sync_tasks_loaded(&self) {
        {
            let cache = self.cache.read().await;
            if cache.sync_tasks_loaded {
                return;
            }
        }
        let data = self
            .load_json_file::<Vec<SyncTask>>("sync_tasks.json")
            .await
            .unwrap_or_default();
        let mut cache = self.cache.write().await;
        if cache.sync_tasks_loaded {
            return;
        }
        cache.sync_tasks = data;
        cache.sync_tasks_loaded = true;
        tracing::debug!(
            count = cache.sync_tasks.len(),
            "Loaded sync tasks on demand"
        );
    }

    async fn ensure_ai_config_loaded(&self) {
        {
            let cache = self.cache.read().await;
            if cache.ai_config_loaded {
                return;
            }
        }
        let data = self
            .load_encrypted_json::<AiProviderConfig>("ai_config.enc")
            .await
            .ok();
        let mut cache = self.cache.write().await;
        if cache.ai_config_loaded {
            return;
        }
        cache.ai_config = data;
        cache.ai_config_loaded = true;
        tracing::debug!(
            present = cache.ai_config.is_some(),
            "Loaded AI config on demand"
        );
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
            if let Some(ref mut ssh) = conn.ssh_tunnel {
                if let Some(enc) = &ssh.password {
                    match self.decrypt(enc) {
                        Ok(plain) => ssh.password = Some(plain),
                        Err(e) => {
                            tracing::warn!(
                                conn_name = %conn.name,
                                error = %e,
                                "Failed to decrypt SSH password, clearing"
                            );
                            ssh.password = None;
                        }
                    }
                }
                if let Some(enc) = &ssh.passphrase {
                    match self.decrypt(enc) {
                        Ok(plain) => ssh.passphrase = Some(plain),
                        Err(e) => {
                            tracing::warn!(
                                conn_name = %conn.name,
                                error = %e,
                                "Failed to decrypt SSH passphrase, clearing"
                            );
                            ssh.passphrase = None;
                        }
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

    async fn write_file_atomic(path: &std::path::Path, content: impl AsRef<[u8]>) -> Result<(), StoreError> {
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
        let tmp_path = parent.join(format!(".{file_name}.tmp"));

        tokio::fs::write(&tmp_path, content.as_ref())
            .await
            .map_err(|e| StoreError::WriteError(e.to_string()))?;
        tokio::fs::rename(&tmp_path, path)
            .await
            .map_err(|e| StoreError::WriteError(e.to_string()))?;

        Ok(())
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

        Self::write_file_atomic(&path, content).await
    }

    async fn persist_connections(&self, connections: &[ConnectionConfig]) -> Result<(), StoreError> {
        let mut to_disk = Vec::with_capacity(connections.len());

        for conn in connections {
            let mut c = conn.clone();
            if let Some(pw) = &c.password {
                c.password = Some(self.encrypt(pw)?);
            }
            if let Some(ref mut ssh) = c.ssh_tunnel {
                if let Some(pw) = &ssh.password {
                    if !pw.is_empty() {
                        ssh.password = Some(self.encrypt(pw)?);
                    }
                }
                if let Some(pp) = &ssh.passphrase {
                    if !pp.is_empty() {
                        ssh.passphrase = Some(self.encrypt(pp)?);
                    }
                }
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
        self.ensure_query_history_loaded().await;
        {
            let mut cache = self.cache.write().await;
            let dominated = cache
                .query_history
                .first()
                .map(|last| last.sql.trim() == entry.sql.trim())
                .unwrap_or(false);
            if dominated {
                if let Some(first) = cache.query_history.first_mut() {
                    first.executed_at = entry.executed_at;
                    first.execution_time_ms = entry.execution_time_ms;
                    first.rows_affected = entry.rows_affected;
                    first.success = entry.success;
                    first.error_message = entry.error_message.clone();
                }
            } else {
                cache.query_history.insert(0, entry);
                if cache.query_history.len() > 1000 {
                    cache.query_history.truncate(1000);
                }
            }
        }

        let snapshot = {
            let cache = self.cache.read().await;
            cache.query_history.clone()
        };

        self.save_json_file("history/queries.json", &snapshot).await
    }

    pub async fn get_query_history(&self, limit: usize) -> Vec<QueryHistoryEntry> {
        self.ensure_query_history_loaded().await;
        let cache = self.cache.read().await;
        cache.query_history.iter().take(limit).cloned().collect()
    }

    pub async fn clear_query_history(&self) -> Result<(), StoreError> {
        self.ensure_query_history_loaded().await;
        {
            let mut cache = self.cache.write().await;
            cache.query_history.clear();
            cache.query_history_loaded = true;
        }
        self.save_json_file("history/queries.json", &Vec::<QueryHistoryEntry>::new())
            .await
    }

    pub async fn get_favorite_queries(&self) -> Vec<FavoriteQuery> {
        self.ensure_favorite_queries_loaded().await;
        let cache = self.cache.read().await;
        cache.favorite_queries.clone()
    }

    pub async fn add_favorite_query(&self, fav: FavoriteQuery) -> Result<(), StoreError> {
        self.ensure_favorite_queries_loaded().await;
        {
            let mut cache = self.cache.write().await;
            cache.favorite_queries.insert(0, fav);
        }
        let snapshot = {
            let cache = self.cache.read().await;
            cache.favorite_queries.clone()
        };
        self.save_json_file("favorites/queries.json", &snapshot).await
    }

    pub async fn delete_favorite_query(&self, id: &str) -> Result<(), StoreError> {
        self.ensure_favorite_queries_loaded().await;
        {
            let mut cache = self.cache.write().await;
            cache.favorite_queries.retain(|f| f.id != id);
        }
        let snapshot = {
            let cache = self.cache.read().await;
            cache.favorite_queries.clone()
        };
        self.save_json_file("favorites/queries.json", &snapshot).await
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

    // ── Sync tasks ──

    pub async fn get_sync_tasks(&self) -> Vec<SyncTask> {
        self.ensure_sync_tasks_loaded().await;
        let cache = self.cache.read().await;
        cache.sync_tasks.clone()
    }

    pub async fn save_sync_task(&self, task: SyncTask) -> Result<(), StoreError> {
        self.ensure_sync_tasks_loaded().await;
        {
            let mut cache = self.cache.write().await;
            if let Some(pos) = cache.sync_tasks.iter().position(|t| t.id == task.id) {
                cache.sync_tasks[pos] = task;
            } else {
                cache.sync_tasks.push(task);
            }
        }
        let snapshot = {
            let cache = self.cache.read().await;
            cache.sync_tasks.clone()
        };
        self.save_json_file("sync_tasks.json", &snapshot).await
    }

    pub async fn delete_sync_task(&self, id: &str) -> Result<(), StoreError> {
        self.ensure_sync_tasks_loaded().await;
        {
            let mut cache = self.cache.write().await;
            cache.sync_tasks.retain(|t| t.id != id);
        }
        let snapshot = {
            let cache = self.cache.read().await;
            cache.sync_tasks.clone()
        };
        self.save_json_file("sync_tasks.json", &snapshot).await
    }

    // ── AI config (encrypted) ──

    pub async fn get_ai_config(&self) -> Option<AiProviderConfig> {
        self.ensure_ai_config_loaded().await;
        let cache = self.cache.read().await;
        cache.ai_config.clone()
    }

    pub async fn save_ai_config(&self, config: &AiProviderConfig) -> Result<(), StoreError> {
        {
            let mut cache = self.cache.write().await;
            cache.ai_config = Some(config.clone());
            cache.ai_config_loaded = true;
        }
        self.save_encrypted_json("ai_config.enc", config).await
    }

    pub async fn delete_ai_config(&self) -> Result<(), StoreError> {
        {
            let mut cache = self.cache.write().await;
            cache.ai_config = None;
            cache.ai_config_loaded = true;
        }
        let path = self.data_dir.join("ai_config.enc");
        if path.exists() {
            tokio::fs::remove_file(&path)
                .await
                .map_err(|e| StoreError::WriteError(e.to_string()))?;
        }
        Ok(())
    }

    // ── Encrypted JSON helpers ──

    async fn load_encrypted_json<T>(&self, filename: &str) -> Result<T, StoreError>
    where
        T: for<'de> Deserialize<'de>,
    {
        let path = self.data_dir.join(filename);
        if !path.exists() {
            return Err(StoreError::ReadError("missing".into()));
        }
        let encrypted = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| StoreError::ReadError(e.to_string()))?;
        let plaintext = self.decrypt(&encrypted)?;
        serde_json::from_str(&plaintext).map_err(|e| StoreError::ParseError(e.to_string()))
    }

    async fn save_encrypted_json<T: Serialize>(
        &self,
        filename: &str,
        data: &T,
    ) -> Result<(), StoreError> {
        let path = self.data_dir.join(filename);
        let json =
            serde_json::to_string(data).map_err(|e| StoreError::ParseError(e.to_string()))?;
        let encrypted = self.encrypt(&json)?;
        Self::write_file_atomic(&path, encrypted).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{ConnectionConfig, SslMode, SshTunnelConfig};

    fn use_file_key_backend() {
        std::env::set_var("DATAZEN_KEYRING", "file");
    }

    async fn init_store_for_test(dir: &std::path::Path) -> Store {
        use_file_key_backend();
        Store::init_with_path(dir).await.unwrap()
    }

    #[tokio::test]
    async fn file_backend_creates_and_reloads_key() {
        use_file_key_backend();
        let dir = tempfile::tempdir().unwrap();
        let k1 = Store::get_or_create_encryption_key_for_test(dir.path())
            .await
            .unwrap();
        let k2 = Store::get_or_create_encryption_key_for_test(dir.path())
            .await
            .unwrap();
        assert_eq!(k1, k2);
        assert!(key_store::key_file_path(dir.path()).is_file());
    }

    #[tokio::test]
    #[ignore = "requires OS keychain; run with: cargo test migrates_dot_key -- --ignored"]
    async fn migrates_dot_key_into_keyring_and_deletes_file() {
        std::env::remove_var("DATAZEN_KEYRING");
        if !key_store::keyring_is_available() {
            eprintln!("skip: OS keychain unavailable");
            return;
        }
        key_store::delete_keyring_entry_for_test();

        let dir = tempfile::tempdir().unwrap();
        let known_key = [7u8; 32];
        std::fs::write(
            key_store::key_file_path(dir.path()),
            BASE64.encode(known_key),
        )
        .unwrap();

        let loaded = key_store::load_or_create_master_key(dir.path()).unwrap();
        assert_eq!(loaded, known_key);
        assert!(!key_store::key_file_path(dir.path()).exists());

        key_store::delete_keyring_entry_for_test();
    }

    fn sample_connection_with_ssh() -> ConnectionConfig {
        ConnectionConfig {
            id: "test-ssh-1".into(),
            name: "SSH Test".into(),
            database_type: "postgresql".into(),
            host: Some("localhost".into()),
            port: Some(5432),
            database: Some("mydb".into()),
            schema: None,
            username: Some("dbuser".into()),
            password: Some("db-secret".into()),
            ssl_mode: SslMode::Disable,
            connection_timeout: 30,
            ssh_tunnel: Some(SshTunnelConfig {
                enabled: true,
                host: "jump.example.com".into(),
                port: 22,
                username: "sshuser".into(),
                auth_method: "password".into(),
                password: Some("ssh-secret-password".into()),
                private_key_path: None,
                passphrase: Some("key-passphrase".into()),
            }),
            color_tag: None,
            group: None,
            last_connected_at: None,
            server_version: None,
            options: None,
        }
    }

    #[tokio::test]
    async fn ssh_credentials_encrypted_on_disk_and_decrypted_in_memory() {
        let dir = tempfile::tempdir().unwrap();
        let store = init_store_for_test(dir.path()).await;

        store
            .save_connection(sample_connection_with_ssh())
            .await
            .unwrap();

        let disk_content =
            tokio::fs::read_to_string(dir.path().join("connections.json"))
                .await
                .unwrap();
        assert!(
            !disk_content.contains("ssh-secret-password"),
            "SSH password must not appear plaintext on disk"
        );
        assert!(
            !disk_content.contains("key-passphrase"),
            "SSH passphrase must not appear plaintext on disk"
        );

        let loaded = store.get_connections().await;
        assert_eq!(loaded.len(), 1);
        let ssh = loaded[0].ssh_tunnel.as_ref().unwrap();
        assert_eq!(ssh.password.as_deref(), Some("ssh-secret-password"));
        assert_eq!(ssh.passphrase.as_deref(), Some("key-passphrase"));
    }

    #[tokio::test]
    async fn connection_options_persist_and_reload() {
        let dir = tempfile::tempdir().unwrap();
        let store = init_store_for_test(dir.path()).await;

        let mut conn = sample_connection_with_ssh();
        let mut opts = serde_json::Map::new();
        opts.insert("topology".into(), serde_json::json!("cluster"));
        conn.options = Some(opts);
        store.save_connection(conn).await.unwrap();

        let loaded = store.get_connections().await;
        assert_eq!(loaded.len(), 1);
        assert_eq!(
            loaded[0].options.as_ref().unwrap()["topology"],
            serde_json::json!("cluster")
        );

        let store2 = init_store_for_test(dir.path()).await;
        let reloaded = store2.get_connections().await;
        assert_eq!(
            reloaded[0].options.as_ref().unwrap()["topology"],
            serde_json::json!("cluster")
        );
    }

    #[tokio::test]
    async fn ssh_credentials_roundtrip_after_reload() {
        let dir = tempfile::tempdir().unwrap();

        {
            let store = init_store_for_test(dir.path()).await;
            store
                .save_connection(sample_connection_with_ssh())
                .await
                .unwrap();
        }

        let store = init_store_for_test(dir.path()).await;
        let loaded = store.get_connections().await;
        assert_eq!(loaded.len(), 1);
        let ssh = loaded[0].ssh_tunnel.as_ref().unwrap();
        assert_eq!(ssh.password.as_deref(), Some("ssh-secret-password"));
        assert_eq!(ssh.passphrase.as_deref(), Some("key-passphrase"));
    }

    #[test]
    fn default_app_data_dir_uses_bundle_identifier() {
        let dir = Store::default_app_data_dir().unwrap();
        assert!(
            dir.ends_with(APP_IDENTIFIER),
            "expected path ending with {APP_IDENTIFIER}, got {}",
            dir.display()
        );
    }

    #[test]
    fn default_app_data_dir_matches_resolve_log_settings_path() {
        let store_dir = Store::default_app_data_dir().unwrap();
        let log_dir = dirs::data_dir()
            .map(|d| d.join(APP_IDENTIFIER))
            .expect("data dir");
        assert_eq!(store_dir, log_dir);
    }

    #[test]
    fn theme_deserializes_legacy_string_and_object() {
        #[derive(Deserialize)]
        struct ThemeField {
            #[serde(deserialize_with = "deserialize_theme", default)]
            theme: ThemePreference,
        }

        let legacy: ThemeField = serde_json::from_str(r#"{"theme":"dark"}"#).unwrap();
        assert_eq!(
            legacy.theme,
            ThemePreference {
                mode: "dark".into(),
                pack_id: None,
            }
        );

        let nested: ThemeField =
            serde_json::from_str(r#"{"theme":{"mode":"dark","packId":null}}"#).unwrap();
        assert_eq!(
            nested.theme,
            ThemePreference {
                mode: "dark".into(),
                pack_id: None,
            }
        );
    }

    #[test]
    fn default_language_is_english() {
        assert_eq!(AppSettings::default().language, "en");
    }

    #[test]
    fn first_run_language_is_supported() {
        let settings = AppSettings::default_for_first_run();
        const OK: &[&str] = &[
            "en", "zh-CN", "zh-TW", "es", "fr", "de", "ja", "pt-BR", "ru", "ko",
        ];
        assert!(
            OK.contains(&settings.language.as_str()),
            "unexpected {}",
            settings.language
        );
    }

    #[test]
    fn plugin_settings_defaults_when_key_missing() {
        let mut value = serde_json::to_value(AppSettings::default()).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .remove("pluginSettings");
        let parsed: AppSettings = serde_json::from_value(value).unwrap();
        assert!(parsed.plugin_settings.is_empty());
    }

    #[test]
    fn plugin_settings_roundtrip_opaque() {
        let settings = AppSettings {
            plugin_settings: {
                let mut m = serde_json::Map::new();
                m.insert("redis".into(), serde_json::json!({ "allowFlush": true }));
                m
            },
            ..AppSettings::default()
        };
        let json = serde_json::to_string(&settings).unwrap();
        assert!(json.contains("pluginSettings"));
        let parsed: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.plugin_settings.get("redis").unwrap()["allowFlush"], true);
    }

    #[test]
    fn settings_json_roundtrip_preserves_language() {
        let settings = AppSettings {
            language: "de".into(),
            ..AppSettings::default()
        };
        let json = serde_json::to_string(&settings).unwrap();
        let parsed: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.language, "de");
    }

    #[test]
    fn monitor_settings_json_roundtrip() {
        use crate::dashboard::types::MonitorSettings;

        let settings = AppSettings {
            monitor: MonitorSettings {
                max_concurrent_queries: 4,
                run_retention_count: 100,
                tray_enabled: false,
                ..MonitorSettings::default()
            },
            ..AppSettings::default()
        };
        let json = serde_json::to_string(&settings).unwrap();
        let parsed: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.monitor.max_concurrent_queries, 4);
        assert_eq!(parsed.monitor.run_retention_count, 100);
        assert!(!parsed.monitor.tray_enabled);
        assert!(parsed.monitor.close_to_tray);
    }

    #[tokio::test]
    async fn save_json_file_leaves_no_tmp_artifacts() {
        let dir = tempfile::tempdir().unwrap();
        let store = init_store_for_test(dir.path()).await;
        let settings = AppSettings {
            language: "fr".into(),
            ..AppSettings::default()
        };
        store.save_settings(settings).await.unwrap();

        let entries: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(!entries.iter().any(|n| n.contains(".tmp")));
        let content = tokio::fs::read_to_string(dir.path().join("settings.json"))
            .await
            .unwrap();
        let parsed: AppSettings = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed.language, "fr");
    }

    #[tokio::test]
    async fn save_ai_config_uses_atomic_encrypted_write() {
        use datazen_ai_api::{AiProviderConfig, AiProviderType};

        let dir = tempfile::tempdir().unwrap();
        let store = init_store_for_test(dir.path()).await;
        let config = AiProviderConfig {
            provider_type: AiProviderType::OpenAi,
            api_key: Some("sk-test".into()),
            endpoint: None,
            model: "gpt-4o".into(),
            max_tokens: 200_000,
            extra: serde_json::Value::Null,
        };
        store.save_ai_config(&config).await.unwrap();

        let entries: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(entries.contains(&"ai_config.enc".to_string()));
        assert!(!entries.iter().any(|n| n.contains(".tmp")));
        let loaded = store.get_ai_config().await.unwrap();
        assert_eq!(loaded.model, "gpt-4o");
    }
}
