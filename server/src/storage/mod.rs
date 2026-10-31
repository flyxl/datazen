//! 服务端管理库（WR-D2 第一片）：SQLite 落地与迁移执行器。
//!
//! 权威与计划的一处已记录差异：[team-server §10.1](../../../../../docs/architecture/platform/team-server-and-auth.md)
//! 与[系统概要 §6.4](../../../../../docs/architecture/platform/system-overview.md)
//! **推荐**生产首版用 PostgreSQL 管理库；web-ready 计划（WR-D2，用户已确认）
//! 裁决服务端**首版先落 SQLite**——同一 domain trait、另一适配器，先把
//! HTTP 宿主立起来，PostgreSQL 适配器是后续生产化切片（届时只新增一个
//! adapter，trait 语义不动）。两份文档的差异已在 web-ready-plan §11 登记。
//!
//! 存储纪律（§10.1 的既定裁决，与选型无关）：
//!
//! - **不复用宿主 AES 落盘实现**：凭据永不落本库明文，行内只有 `SecretRef`
//!   引用；解析是 `SecretProvider` 的事（WR-D4）。
//! - **expand/contract**：迁移只增列增表，不先删字段；contract 步骤必须
//!   在备份可核验恢复后才执行（本执行器只跑 expand，contract 显式拒绝）。
//! - **组织隔离**（CM-05）：所有查询按 `organization_id` 限定；越组织读取
//!   返回 `None` / 空列表，不报错、不泄漏「存在但无权」。
//!
//! 并发模型与桌面 `AppDb` 同款：单 `Mutex<Connection>`，异步边界由适配器
//! 用 `spawn_blocking` 包住（内核 trait 是 async，SQLite 是同步——差异
//! 只允许出现在这一层）。

pub mod known_hosts;
pub mod migrations;
pub mod profiles;

pub use known_hosts::ServerKnownHosts;
pub use profiles::ServerProfiles;

#[cfg(test)]
mod known_hosts_tests;
#[cfg(test)]
mod profiles_tests;

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::Connection;

/// 管理库打开/迁移失败。进程边界事实：只进日志与 stderr（同 `ServerError`
/// 的纪律），进端口层时映射为 `PortError::BackendUnavailable`，不携带 SQL
/// 原文之外的值。
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("admin store open failed: {0}")]
    Open(String),

    #[error("migration failed at version {version} ({name}): {source}")]
    Migration {
        version: i64,
        name: &'static str,
        source: String,
    },

    #[error("admin store query failed: {0}")]
    Query(String),
}

impl From<rusqlite::Error> for StorageError {
    fn from(source: rusqlite::Error) -> Self {
        StorageError::Query(source.to_string())
    }
}

/// 已应用的迁移记录（`migrate` 子命令的输出单位）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedMigration {
    pub version: i64,
    pub name: &'static str,
    pub applied_at: String,
}

/// 服务端管理库：一个 SQLite 文件。
pub struct AdminStore {
    conn: Mutex<Connection>,
    #[allow(dead_code)]
    path: Option<PathBuf>,
}

impl AdminStore {
    /// 打开（或创建）管理库文件。WAL + foreign_keys + busy_timeout 是
    /// 打开期不变式，不留给调用方。
    pub fn open(path: &Path) -> Result<Self, StorageError> {
        let conn = Connection::open(path)
            .map_err(|source| StorageError::Open(format!("{}: {source}", path.display())))?;
        Self::configure(conn).map(|conn| Self {
            conn: Mutex::new(conn),
            path: Some(path.to_path_buf()),
        })
    }

    /// 内存库（测试与 `check-schema` 的只读探查）。
    pub fn open_in_memory() -> Result<Self, StorageError> {
        let conn = Connection::open_in_memory()
            .map_err(|source| StorageError::Open(format!("in-memory: {source}")))?;
        Self::configure(conn).map(|conn| Self {
            conn: Mutex::new(conn),
            path: None,
        })
    }

    fn configure(conn: Connection) -> Result<Connection, StorageError> {
        // WAL 对内存库无意义但无害（返回 "memory"）；busy_timeout 缓解
        // 与 `migrate` 子命令的短暂并发。
        let _ = conn.pragma_update(None, "journal_mode", "WAL");
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        Ok(conn)
    }

    /// 应用所有未应用的迁移（只跑 expand）。每个迁移单事务：DDL + 版本行
    /// 要么都落要么都不落。
    pub fn migrate(&self) -> Result<Vec<AppliedMigration>, StorageError> {
        self.ensure_version_table()?;
        let mut applied = Vec::new();
        for migration in migrations::MIGRATIONS {
            if self.version_applied(migration.version)? {
                continue;
            }
            let at = chrono::Utc::now().to_rfc3339();
            {
                let conn = self.lock()?;
                let tx = conn.unchecked_transaction().map_err(|source| {
                    StorageError::Migration {
                        version: migration.version,
                        name: migration.name,
                        source: source.to_string(),
                    }
                })?;
                tx.execute_batch(migration.expand).map_err(|source| {
                    StorageError::Migration {
                        version: migration.version,
                        name: migration.name,
                        source: source.to_string(),
                    }
                })?;
                tx.execute(
                    "INSERT INTO admin_schema_version (version, name, applied_at) VALUES (?1, ?2, ?3)",
                    rusqlite::params![migration.version, migration.name, at],
                )
                .map_err(|source| StorageError::Migration {
                    version: migration.version,
                    name: migration.name,
                    source: source.to_string(),
                })?;
                tx.commit().map_err(|source| StorageError::Migration {
                    version: migration.version,
                    name: migration.name,
                    source: source.to_string(),
                })?;
            }
            applied.push(AppliedMigration {
                version: migration.version,
                name: migration.name,
                applied_at: at,
            });
        }
        Ok(applied)
    }

    /// 当前库的 schema 版本；空库为 0。
    pub fn schema_version(&self) -> Result<i64, StorageError> {
        let has_table: bool = self.lock()?.query_row(
            "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'admin_schema_version')",
            [],
            |row| row.get(0),
        )?;
        if !has_table {
            return Ok(0);
        }
        self.lock()?.query_row(
            "SELECT COALESCE(MAX(version), 0) FROM admin_schema_version",
            [],
            |row| row.get(0),
        )
    }

    fn ensure_version_table(&self) -> Result<(), StorageError> {
        self.lock()?.execute_batch(
            "CREATE TABLE IF NOT EXISTS admin_schema_version (
               version INTEGER PRIMARY KEY,
               name TEXT NOT NULL,
               applied_at TEXT NOT NULL
             );",
        )?;
        Ok(())
    }

    fn version_applied(&self, version: i64) -> Result<bool, StorageError> {
        self.lock()?.query_row(
            "SELECT EXISTS (SELECT 1 FROM admin_schema_version WHERE version = ?1)",
            [version],
            |row| row.get(0),
        )
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>, StorageError> {
        self.conn
            .lock()
            .map_err(|_| StorageError::Query("admin store lock poisoned".into()))
    }

    /// 适配器（`storage::profiles` 等）的受控锁访问：同步核心直接复用
    /// 同一条连接，不另开池（单文件 SQLite 的既定并发模型）。
    pub(crate) fn lock_for_adapter(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, Connection>, StorageError> {
        self.lock()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_is_idempotent_and_reports_first_run_only() {
        let store = AdminStore::open_in_memory().unwrap();
        let first = store.migrate().unwrap();
        assert_eq!(first.len(), migrations::MIGRATIONS.len());
        let second = store.migrate().unwrap();
        assert!(second.is_empty(), "second run must apply nothing");
        assert_eq!(
            store.schema_version().unwrap(),
            migrations::MIGRATIONS.last().map(|m| m.version).unwrap_or(0)
        );
    }

    #[test]
    fn empty_store_reports_version_zero() {
        let store = AdminStore::open_in_memory().unwrap();
        assert_eq!(store.schema_version().unwrap(), 0);
    }
}
