# 持久化存储

> [返回架构总览](../README.md)

### 1.1 存储架构

```rust
// src-tauri/src/store/mod.rs

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use rand::RngCore;

/// 存储管理器
pub struct Store {
    /// 存储目录
    data_dir: PathBuf,
    /// 加密密钥 (从系统密钥链获取)
    encryption_key: [u8; 32],
    /// 内存缓存
    cache: Arc<RwLock<StoreCache>>,
}

#[derive(Default)]
struct StoreCache {
    connections: Vec<ConnectionConfig>,
    settings: AppSettings,
    query_history: Vec<QueryHistoryEntry>,
    favorites: Vec<FavoriteQuery>,
}

/// 浅色 / 深色 / 跟随系统，以及可选的已安装主题包 ID
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ThemePreference {
    pub mode: String,
    #[serde(default)]
    pub pack_id: Option<String>,
}

/// 应用设置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    #[serde(deserialize_with = "deserialize_theme", default)]
    pub theme: ThemePreference,
    pub language: String,
    pub query_result_limit: u32,
    pub auto_save: bool,
    pub confirm_on_delete: bool,
    pub editor_font_size: u32,
    pub editor_font_family: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: ThemePreference {
                mode: "dark".into(),
                pack_id: None,
            },
            language: "zh-CN".to_string(),
            query_result_limit: 1000,
            auto_save: true,
            confirm_on_delete: true,
            editor_font_size: 13,
            editor_font_family: "JetBrains Mono".to_string(),
        }
    }
}

/// 查询历史条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryHistoryEntry {
    pub id: String,
    pub connection_id: String,
    pub database: String,
    pub sql: String,
    pub executed_at: chrono::DateTime<chrono::Utc>,
    pub execution_time_ms: u64,
    pub rows_affected: Option<u64>,
    pub success: bool,
    pub error_message: Option<String>,
}

/// 收藏的查询
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FavoriteQuery {
    pub id: String,
    pub name: String,
    pub connection_id: Option<String>,
    pub database: Option<String>,
    pub sql: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub tags: Vec<String>,
}

impl Store {
    /// 初始化存储
    pub async fn init(app_handle: &tauri::AppHandle) -> Result<Self, StoreError> {
        // 获取应用数据目录
        let data_dir = app_handle
            .path()
            .app_data_dir()
            .map_err(|e| StoreError::InitError(e.to_string()))?;
        
        // 确保目录存在
        tokio::fs::create_dir_all(&data_dir)
            .await
            .map_err(|e| StoreError::InitError(e.to_string()))?;
        
        // 获取或创建加密密钥
        let encryption_key = Self::get_or_create_encryption_key(&data_dir).await?;
        
        let store = Self {
            data_dir,
            encryption_key,
            cache: Arc::new(RwLock::new(StoreCache::default())),
        };
        
        // 加载已有数据
        store.load_all().await?;
        
        Ok(store)
    }
    
    /// 从系统密钥链获取或创建加密密钥
    async fn get_or_create_encryption_key(data_dir: &PathBuf) -> Result<[u8; 32], StoreError> {
        // 尝试从 keyring 获取
        let keyring = keyring::Entry::new("DataZen", "encryption_key");
        
        match keyring.get_password() {
            Ok(key_b64) => {
                // 解码现有密钥
                let key_bytes = BASE64.decode(&key_b64)
                    .map_err(|e| StoreError::EncryptionError(e.to_string()))?;
                
                let mut key = [0u8; 32];
                key.copy_from_slice(&key_bytes);
                Ok(key)
            }
            Err(_) => {
                // 生成新密钥
                let mut key = [0u8; 32];
                OsRng.fill_bytes(&mut key);
                
                // 存储到 keyring
                let key_b64 = BASE64.encode(&key);
                keyring.set_password(&key_b64)
                    .map_err(|e| StoreError::EncryptionError(e.to_string()))?;
                
                Ok(key)
            }
        }
    }
    
    /// 加密数据
    fn encrypt(&self, plaintext: &str) -> Result<String, StoreError> {
        let cipher = Aes256Gcm::new_from_slice(&self.encryption_key)
            .map_err(|e| StoreError::EncryptionError(e.to_string()))?;
        
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        
        let ciphertext = cipher
            .encrypt(nonce, plaintext.as_bytes())
            .map_err(|e| StoreError::EncryptionError(e.to_string()))?;
        
        // 格式: base64(nonce || ciphertext)
        let mut combined = nonce_bytes.to_vec();
        combined.extend(ciphertext);
        
        Ok(BASE64.encode(&combined))
    }
    
    /// 解密数据
    fn decrypt(&self, encrypted: &str) -> Result<String, StoreError> {
        let combined = BASE64.decode(encrypted)
            .map_err(|e| StoreError::EncryptionError(e.to_string()))?;
        
        if combined.len() < 12 {
            return Err(StoreError::EncryptionError("Invalid encrypted data".to_string()));
        }
        
        let (nonce_bytes, ciphertext) = combined.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);
        
        let cipher = Aes256Gcm::new_from_slice(&self.encryption_key)
            .map_err(|e| StoreError::EncryptionError(e.to_string()))?;
        
        let plaintext = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| StoreError::EncryptionError(e.to_string()))?;
        
        String::from_utf8(plaintext)
            .map_err(|e| StoreError::EncryptionError(e.to_string()))
    }
    
    /// 加载所有数据
    async fn load_all(&self) -> Result<(), StoreError> {
        let mut cache = self.cache.write().await;
        
        // 加载连接配置
        cache.connections = self.load_json_file("connections.json")
            .await
            .unwrap_or_default();
        
        // 解密密码
        for conn in &mut cache.connections {
            if let Some(encrypted) = &conn.password {
                conn.password = Some(self.decrypt(encrypted)?);
            }
        }
        
        // 加载设置
        cache.settings = self.load_json_file("settings.json")
            .await
            .unwrap_or_default();
        
        // 加载查询历史
        cache.query_history = self.load_json_file("history/queries.json")
            .await
            .unwrap_or_default();
        
        // 加载收藏
        cache.favorites = self.load_json_file("favorites/queries.json")
            .await
            .unwrap_or_default();
        
        Ok(())
    }
    
    /// 加载 JSON 文件
    async fn load_json_file<T: for<'de> Deserialize<'de>>(&self, filename: &str) -> Result<T, StoreError> {
        let path = self.data_dir.join(filename);
        
        if !path.exists() {
            return Err(StoreError::FileNotFound(filename.to_string()));
        }
        
        let content = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| StoreError::ReadError(e.to_string()))?;
        
        serde_json::from_str(&content)
            .map_err(|e| StoreError::ParseError(e.to_string()))
    }
    
    /// 保存 JSON 文件
    async fn save_json_file<T: Serialize>(&self, filename: &str, data: &T) -> Result<(), StoreError> {
        let path = self.data_dir.join(filename);
        
        // 确保父目录存在
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| StoreError::WriteError(e.to_string()))?;
        }
        
        let content = serde_json::to_string_pretty(data)
            .map_err(|e| StoreError::ParseError(e.to_string()))?;
        
        tokio::fs::write(&path, content)
            .await
            .map_err(|e| StoreError::WriteError(e.to_string()))?;
        
        Ok(())
    }
}

/// 连接配置存储服务
impl Store {
    /// 获取所有连接配置
    pub async fn get_connections(&self) -> Vec<ConnectionConfig> {
        let cache = self.cache.read().await;
        cache.connections.clone()
    }
    
    /// 获取单个连接配置
    pub async fn get_connection(&self, id: &str) -> Option<ConnectionConfig> {
        let cache = self.cache.read().await;
        cache.connections.iter().find(|c| c.id == id).cloned()
    }
    
    /// 保存连接配置
    pub async fn save_connection(&self, config: ConnectionConfig) -> Result<(), StoreError> {
        let mut cache = self.cache.write().await;
        
        // 加密密码
        let mut config = config;
        if let Some(password) = &config.password {
            config.password = Some(self.encrypt(password)?);
        }
        
        // 更新或添加
        if let Some(pos) = cache.connections.iter().position(|c| c.id == config.id) {
            cache.connections[pos] = config;
        } else {
            cache.connections.push(config);
        }
        
        // 保存到文件
        self.save_json_file("connections.json", &cache.connections).await?;
        
        Ok(())
    }
    
    /// 删除连接配置
    pub async fn delete_connection(&self, id: &str) -> Result<(), StoreError> {
        let mut cache = self.cache.write().await;
        
        cache.connections.retain(|c| c.id != id);
        
        self.save_json_file("connections.json", &cache.connections).await?;
        
        Ok(())
    }
    
    /// 解密密码 (供 ConnectionManager 使用)
    pub fn decrypt_password(&self, encrypted: &str) -> Result<String, StoreError> {
        self.decrypt(encrypted)
    }
}

/// 查询历史管理
impl Store {
    /// 添加查询历史
    pub async fn add_query_history(&self, entry: QueryHistoryEntry) -> Result<(), StoreError> {
        let mut cache = self.cache.write().await;
        
        cache.query_history.insert(0, entry);
        
        // 限制历史记录数量
        if cache.query_history.len() > 1000 {
            cache.query_history.truncate(1000);
        }
        
        self.save_json_file("history/queries.json", &cache.query_history).await?;
        
        Ok(())
    }
    
    /// 获取查询历史
    pub async fn get_query_history(&self, limit: usize) -> Vec<QueryHistoryEntry> {
        let cache = self.cache.read().await;
        cache.query_history.iter().take(limit).cloned().collect()
    }
    
    /// 清空查询历史
    pub async fn clear_query_history(&self) -> Result<(), StoreError> {
        let mut cache = self.cache.write().await;
        cache.query_history.clear();
        
        self.save_json_file("history/queries.json", &cache.query_history).await?;
        
        Ok(())
    }
}

/// 设置管理
impl Store {
    /// 获取设置
    pub async fn get_settings(&self) -> AppSettings {
        let cache = self.cache.read().await;
        cache.settings.clone()
    }
    
    /// 保存设置
    pub async fn save_settings(&self, settings: AppSettings) -> Result<(), StoreError> {
        let mut cache = self.cache.write().await;
        cache.settings = settings;
        
        self.save_json_file("settings.json", &cache.settings).await?;
        
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("Initialization error: {0}")]
    InitError(String),
    
    #[error("File not found: {0}")]
    FileNotFound(String),
    
    #[error("Read error: {0}")]
    ReadError(String),
    
    #[error("Write error: {0}")]
    WriteError(String),
    
    #[error("Parse error: {0}")]
    ParseError(String),
    
    #[error("Encryption error: {0}")]
    EncryptionError(String),
}

/// 类型别名
pub type ConfigStore = Store;
```
