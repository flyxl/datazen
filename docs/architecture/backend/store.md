# 持久化存储

> [返回架构总览](../README.md)

> 下文部分 Rust 片段为**架构示意**；实现以 `src-tauri/src/store/` 为准（主密钥见 `key_store.rs`）。

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
    /// AES-256 master key（来自 OS keychain 或 `{appData}/.key`，见 key_store）
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

/// 收藏的查询 —— 自 §2.6 起每条是一个 `.sql` 文件，id 即文件名（ULID）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FavoriteQuery {
    /// 文件名去扩展名，ULID；删除 / 改名只作用于这一个文件
    pub id: String,
    /// front-matter 的 `connectionId`；无归属时为空串
    pub connection_id: String,
    pub title: String,
    /// front-matter 之后的全部正文，逐字节可执行
    pub sql: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    #[serde(default)]
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
    /// 收藏时选中的库，便于换库后重新绑定
    #[serde(default)]
    pub database: Option<String>,
    /// 预留：关键词补全轨；只解析与回写，本期不做 UI
    #[serde(default)]
    pub keyword: Option<String>,
    /// 相对收藏根目录的 `/` 分隔子目录，根目录为 None
    #[serde(default)]
    pub folder: Option<String>,
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
        
        // 获取或创建主加密密钥（key_store：钥匙串 / `.key`）
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
        
        // 收藏是文件，不在这里加载：见 §1.4 收藏（文件优先）
        // FavoritesStore 自带 RwLock 缓存，列表在调用时按需扫描
        
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

## 主加密密钥（`key_store`）

连接密码、SSH 凭据、`ai_config.enc` 等均用 **AES-256-GCM**；磁盘上存密文。主密钥（32 字节）由 `src-tauri/src/store/key_store.rs` 管理，**不是**写死只用钥匙串或只用文件，而是双后端：

| 后端 | 位置 | 何时使用 |
|------|------|----------|
| **OS Keychain** | macOS Keychain / Windows Credential Manager / Linux Secret Service；账户 `app-encryption-key`（服务标识 `APP_IDENTIFIER`） | 正式签名构建的默认路径；`DATAZEN_KEYRING=keyring` 强制 |
| **文件 `.key`** | `{appData}/.key`（base64 主密钥） | `DATAZEN_KEYRING=file`（dev/CI）；macOS **adhoc/未签名** 二进制（`tauri:dev` 等）自动优先，避免每次重链弹钥匙串 ACL |

选择逻辑摘要：

```text
DATAZEN_KEYRING=file     → File
DATAZEN_KEYRING=keyring  → Keyring
unset + macOS adhoc      → File
unset + 其它             → Keyring（失败可回退已有 `.key`）
```

- 首次启动随机生成主密钥并写入当前后端；已有 `.key` 时可迁移进钥匙串后删除文件。
- 应用数据 ZIP **不包含** `.key`；跨机恢复密文需另行备份主密钥（设置流程可走 `save_encryption_key_with_dialog`）。
- 实现入口：`Store::get_or_create_encryption_key` → `key_store::load_or_create_master_key`。

### 1.3 查询历史（明文）

`{appData}/history.sqlite`（`history_db.rs`）持久化 SQL 查询与 Workflow 执行历史。**SQL 文本、错误信息与库/schema 上下文以明文存储**，未像 `connections.json` / `ai_config.enc` 那样 AES 加密。语句中可能含字面量或片段性敏感数据；备份/同步 app data 目录时需视同敏感审计日志。日志路径已用 `log_redact` 脱敏；history 落盘加密留作后续加固项。

### 1.4 收藏（文件优先）

收藏不再进 SQLite。每条收藏是收藏根目录下的**一个 `.sql` 文件**，文件名是去扩展名的 ULID，因此 `ls` 的时间序就是创建序，`diff` / `merge` / 备份都能交给 iCloud、Dropbox、Git 直接做。实现见 `src-tauri/src/store/favorites/`。

根目录取 `AppSettings.favoritesRoot`，缺省或空串时是 `{appData}/favorites`。**目前没有设置界面**：改动只存在于 `settings.json`，`Store::init_with_path` 在启动时应用一次（解析失败则 `tracing::warn!` 并回落到缺省值），运行中改设置不会改指当前 store，界面上只以 `query.favoritesRoot`（"Saved in"）显示当前解析结果。面板会重扫，不会替用户搬文件。

```text
{appData}/favorites/            ← 可用 AppSettings.favoritesRoot 改指到任意已同步目录
├── 01J8XK2M9Q7B4F.sql
├── reports/
│   └── 01J8XK2MA1C2D3E.sql      ← 子目录递归扫描，folder = "reports"
└── .trash/                      ← 软删除；不参与扫描
    └── 2026-08-18T10-00-00-000__01J8XK2M9Q7B4F.sql
```

**front-matter 用 `--` SQL 注释承载**，与 SQL 注释结构同形，所以文件本身仍可直接执行：

```sql
-- title: Nightly recon
-- connectionId: cfg-1
-- createdAt: 2026-01-01T09:00:00+00:00
-- updatedAt: 2026-01-02T11:20:00+00:00
SELECT * FROM orders WHERE d = CURRENT_DATE;
```

不变量与理由：

- **键序即文件序**。front-matter 是 `Vec<(String, String)>` 而非 `BTreeMap`：按字母序重写会让每次保存都改动所有文件，diff 全是噪声。`render_file` 的键序固定为 `title / connectionId / database / keyword / createdAt / updatedAt`（空值的键不写），因此一条未被编辑的收藏重写后逐字节不变。允许的字符集比 ULID 宽，是为了让用户在访达里改名后收藏仍可编辑。
- **未知键会被丢弃**，这是有意的取舍而非遗漏。读回时只把上面 6 个字段装进 `FavoriteQuery`，重写时也只渲染这 6 个字段，所以**旧版本 App 改写新版本写的文件会丢掉新键**。换来的是格式永远由一处定义；代价只在跨版本混编的同步目录上出现。
- **值转义**。`\n` `\r` `\\` 转义后再落盘，值无法自己换行，front-matter 因此不可能渗进语句正文。反向读取时每个值会被 `trim()`，所以标题末尾的手工空格读回即丢——仅影响显示，读操作从不重写文件。
- **只归一化一处**。`parse` 不改写任何内容（读手写文件不会被重写）；`render` 在正文缺尾换行时补一个 `\n`。所以编辑器里的 `SELECT 1` 读回是 `SELECT 1\n`，这是全部差异；正文本来就以 `\n` 结尾的（哪怕是两个）不会被改动。
- **id 只来自文件名**。写与删走 `FavoritesStore::resolve_file`，它先过 `is_safe_stem`（ASCII 字母数字加 `-` `_`，≤ 64 **字节**，并按大小写不敏感拒绝 22 个 Windows 设备名），不通过即 `UnsafeId`，因此穿越与绝对路径在任何情况下都到不了 `fs`。读方向不走这个门：扫描把文件名的 stem 当 id，所以用户在根目录里手工放一个怪名字的文件不会伤到任何东西，只是 id 不好看。
- **写入是原子的**：临时文件 + `fs::rename`；失败不留临时文件。
- **删除是软删除**：移入 `.trash/{ISO 时间}-{毫秒:03}__{id}.sql`（如 `2026-08-18T10-00-00-000__01J8XK2M9Q7B4F.sql`），人眼可排序且 id 可还原。`.trash` 与一切 `.` 开头目录都不参与扫描。
- **符号链接不跟随**（`symlink_metadata`），避免根目录外的内容被扫进来。
- **扫描有内存缓存**，只有 App 自身的写操作会更新它。同步客户端在运行期间投递的文件要等 UI 显式重扫才会出现——面板在**打开时**和**窗口重新获得焦点时**调用 `refresh_favorites`，并提供手动重扫按钮。这是"缓存 + 显式失效"，不是文件监听器：监听一个用户随时可能指向 iCloud 的目录并不可靠，而重扫一个纯文本目录很便宜。
- **无归属的收藏不会被隐藏或删除**：`connectionId` 缺失时 `connection_id` 为空串，按连接过滤时只在"全部"视图出现。
- **文件本身始终是可执行 SQL**：`tests/executability.rs` 把带 front-matter 的文件原样交给 `rusqlite` 执行，断言结果与只执行语句正文完全一致（正文含 `--` 行注释、`/* */` 块注释，以及字符串字面量里的 `--` 与 `*/`）。

#### 1.4.1 从 `favorite_queries` 表迁移

`favorite_queries` 表已停用（v2 建表语句不再创建它，新装用户根本不会有）。老用户升级后，`Store::init_with_path` 在打开 `HistoryDb` 与 `FavoritesStore`、并应用 `favoritesRoot` 设置之后，执行一次导出：

1. `favorite_queries_legacy_v1` 已存在 → 直接返回，不读任何行（见下）。
2. 表不存在 → 直接返回，不写标记文件。
3. 表存在且**有行**：逐行写成 `.sql` 文件，ULID 由 `Sha256(legacy_id)[0..10]` 确定性导出（时间戳取该行的 `created_at`），因此同一行在任何一次重试里都落到同一路径。**全部写成功之后**才把表重命名为 `favorite_queries_legacy_v1`。
4. 表存在但**为空**：不动 schema，直接返回。

关键性质：

- **幂等的护栏是重命名，不是标记文件。** `.migration-v1.json` 只是给用户和排查用的记录；即便它被同步冲突吞掉，也不可能引起二次导出——因为护栏是「表已经不在原名下了」。反过来，同步冲突**不可能**造成漏导出。
- **归档表在先，因此已迁移的库不会被反复导出。** 从备份恢复、或安装被回滚，可能让一个**陈旧的** `favorite_queries` 与 `favorite_queries_legacy_v1` 同时存在。此时导出早已完成，那张表是复活的数据而非迁移输入；由于 ULID 确定性，重跑不会产生副本，而会**每次启动都把用户改过或删掉的收藏按旧数据覆盖 / 复活**。所以 `migrate_legacy_favorites` 在读任何一行之前先查归档表，命中即返回 `NothingToDo`。
- **失败不消耗任何东西。** 中途写失败则错误上抛、表原样保留（不重命名），下次启动从同一批行重跑，只重写同一批路径：已写好的被覆盖，不产生副本。
- **原数据可回滚。** 表是重命名而非 `DROP`，`favorite_queries_legacy_v1` 里三行俱全。
- **排序不变。** 旧面板是 `ORDER BY created_at DESC`，导出后按 `created_at` 倒序（同毫秒以 id 兜底），一致。

覆盖以上各点的测试在 `src-tauri/src/store/favorites/tests/migration.rs`（`tests.rs` 里的 `mod migration`），全部使用真实 `tempfile` 临时目录；文件可执行性在同目录的 `executability.rs`。把 `ALTER TABLE … RENAME` 换成空操作会让其中 4 个测试转红。

### 1.5 DTO 与持久化格式兼容范围

本节记录**当前已实现**的落盘格式能读什么、能写什么，以及哪些运行时状态**被刻意排除在持久化之外**。

#### 1.5.1 字段级兼容规则

所有落盘 DTO 的序列化键名统一为 camelCase（`#[serde(rename_all = "camelCase")]`），Rust 侧保持 snake_case。缺字段一律靠 `#[serde(default)]` 兜底，因此**旧文件永远读得进来，新字段不需要版本号**：

| DTO | 关键字段 | 兼容规则 |
|---|---|---|
| `QueryHistoryEntry` | `connectionId`（持久化连接 id）、`database`（会话当时的逻辑库，`""` 表示未知 / 旧行）、`schema` | `schema` 为可选且带 `default`；旧行没有 schema 时读作 `None`，不反推 |
| `FavoriteQuery` | `id`（`.sql` 文件名的 stem）、`connectionId`、`updatedAt`、`keyword`、`database`、`folder` | `updatedAt` 为可选且带 `default`：从 SQLite 迁移来的收藏没有这一列。`id` 只来自文件名，改标题不会改文件名 |
| `SyncTask` | `sourceConnectionId` / `targetConnectionId`、`source/targetDatabase`、`source/targetSchema` | 全部 `#[serde(default)]`；连接 id 是**恢复与展示的唯一依据** |
| `SyncTask` | `sourceDbSessionId` / `targetDbSessionId` | `#[serde(default, skip_serializing)]`：**只读不写**。字段留在 Rust 模型里是为了能解析旧 JSON，落盘时一律省略 |

#### 1.5.2 旧 session ID 不是迁移数据

Data Sync 任务文件是唯一曾经写入过运行时 `dbSessionId` 的地方，现在这条边界由三处机制共同保证：

1. **模型层**：两个 session 字段带 `skip_serializing`，序列化时不存在于输出中，因此任何 `save_sync_task` 都不会把它们写回文件。
2. **归一化层**：`SyncTask::normalize_legacy_state` 清空这两个字段；同时认为「从行偏移续跑」在进程重启后不安全，把 `current_table_offset` 归零、`strategy` 置 `unknown`、`status` 置 `interrupted`、`resume_state` 置 `unknown` 并记录一条明确的错误信息。加载路径与保存路径都调用它。
3. **结构层**：迁移运行记录表里根本没有承载 session id 的列——测试直接读表结构断言 `db_session_id` 等敏感列名一个都不存在。

因此：**任务恢复只依据持久化的 `sourceConnectionId` / `targetConnectionId`（以及 database / schema 字段）**，运行时 session ID 既不是迁移输入，也不是迁移输出。Data Transfer 与 Schema Diff 的 profile 同理，落盘记录只含 `connectionId`；Data Transfer 的运行时端点（`dbSessionId` + database + schema）只随任务在内存中流转，从不进入任何 profile 文件。Schema Diff 的 profile 文件本身是加密存储。

#### 1.5.3 不落盘的运行时状态

以下状态**只存在于内存**，跨进程即失效，任何格式变更都不应把它们纳入兼容承诺：

| 状态 | 位置 | 失效表现 |
|---|---|---|
| 会话取消标志 | `services/job_registry.rs` | 进程级表，key 是 job ID；应用重启后旧 job ID 不可取消，且先于开始执行到达的取消标志会被保留 |
| AI 调用取消令牌 | `ai/cancel.rs` | 按 AI 调用注册 / 注销 |
| 查询流执行注册表 | `AppState.query_executions` | 按 `QueryExecutionId` 归属校验；未知或已结束的 ID 一律拒绝 |
| 会话事务句柄 | `AppState.session_transactions` | 键是 `dbSessionId`；只有显式断开才回滚 |

兼容性的验证落在 `src-tauri/src/store/tests/migration_tests.rs`：一个测试写入带 `sourceDbSessionId` / `targetDbSessionId` 与行偏移的任务，断言保存后磁盘 JSON 中**不含**这两个键、重新加载后两者为空且断点被中和；另一个测试直接喂一段手写的旧 JSON 数组，断言同样被中和、而连接 id 存活。
