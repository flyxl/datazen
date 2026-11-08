//! Data Sync 资产的服务端 SQLite 适配器（W8 刀3e-2；v15 `sync_tasks` /
//! `sync_profiles` 双表）。
//!
//! 词汇唯一定义在共享层 `datazen::migration::data_sync`：
//! [`StoredSyncTask`]（`task_checkpoint`，断点/续传状态——W8 刀3e-2 从桌面
//! `store/models.rs` 上移）与 [`SyncProfile`]（`profile`，可复用同步配置）。
//! 桌面实现是两个 JSON 文件（`sync_tasks.json` / `sync_profiles.json`，
//! 单租户全体共享），web 形态改存服务端库行（v11 查询资产「文件形的
//! 服务端对偶」同款裁决）。本件只负责行存取与隔离边界：
//!
//! - **per organization 隔离**：任务与档案都是组织级资产（桌面单租户不
//!   区分归属；服务端按组织收敛，CM-05：越界读空、写只落本边界）。隔离
//!   键来自 [`OrganizationScope`]——由调用方按认证链事实装配（INV-01：
//!   组织不来自请求体）。
//! - **同步核心 + `spawn_blocking` 包装**（settings / query_assets /
//!   schema_diff_profiles 同款异步边界）。
//! - **行形**：档案全列投影（v12 `schema_diff_profiles` 同款——深度结构
//!   `tables_json` / `options_json`）；任务整档 `payload_json` + 续航查找
//!   投影列（连接对/状态/时间，v15 迁移头登记的口径）。坏行按存储故障
//!   上抛，不静默丢弃——服务端行只经本适配器写入，坏行意味着存储损坏。
//! - **遗留归一化在读写两沿执行**：[`StoredSyncTask::normalize_legacy_state`]
//!   清运行时会话 id 与不安全的 OFFSET 检查点（桌面 save 前同款纪律）；
//!   读取时归一化只影响应答投影，不回写行（桌面 load-后-重写是文件形的
//!   一次性迁移，服务端行按写入时归一化收敛，读沿不产生写放大）。

use std::sync::Arc;

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};

use datazen::migration::data_sync::profile::SyncProfile;
use datazen::migration::data_sync::StoredSyncTask;
use datazen::shared::scope::OrganizationScope;

use crate::storage::AdminStore;

/// 存储面错误（调用方映射为 503 `serviceUnavailable`，桌面
/// `CommandError::Internal` 的服务端对应物）。
#[derive(Debug, thiserror::Error)]
pub enum SyncAssetStoreError {
    #[error("sync asset store: {0}")]
    Backend(String),
}

fn unavailable(detail: impl std::fmt::Display) -> SyncAssetStoreError {
    SyncAssetStoreError::Backend(detail.to_string())
}

/// v15 `sync_tasks` 表的行存取（任务断点/续传状态）。
#[derive(Clone)]
pub struct ServerSyncTasks {
    store: Arc<AdminStore>,
}

impl ServerSyncTasks {
    pub fn new(store: Arc<AdminStore>) -> Self {
        Self { store }
    }

    fn list_sync(
        conn: &Connection,
        scope: &OrganizationScope,
    ) -> Result<Vec<StoredSyncTask>, SyncAssetStoreError> {
        let mut stmt = conn
            .prepare(
                "SELECT payload_json FROM sync_tasks WHERE organization_id = ?1 \
                 ORDER BY created_at, id",
            )
            .map_err(unavailable)?;
        let rows = stmt
            .query_map(params![scope.organization_id.as_str()], |row| {
                let payload: String = row.get(0)?;
                Ok(payload)
            })
            .map_err(unavailable)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(unavailable)?;
        rows.iter()
            .map(|payload| {
                let mut task: StoredSyncTask = serde_json::from_str(payload)
                    .map_err(|error| unavailable(format!("decode sync task row: {error}")))?;
                // 读沿归一化：遗留检查点在应答里如实降级（interrupted +
                // 不可续传文案），行不回写（见模块头）。
                task.normalize_legacy_state();
                Ok(task)
            })
            .collect()
    }

    fn save_sync(
        conn: &Connection,
        scope: &OrganizationScope,
        task: &StoredSyncTask,
    ) -> Result<(), SyncAssetStoreError> {
        // 写沿归一化（桌面 save 前同款）：运行时会话与不安全检查点
        // 永不落库。
        let mut normalized = task.clone();
        normalized.normalize_legacy_state();
        let payload = serde_json::to_string(&normalized)
            .map_err(|error| unavailable(format!("encode sync task row: {error}")))?;
        conn.execute(
            "INSERT INTO sync_tasks (
               organization_id, id, source_connection_id, target_connection_id,
               status, payload_json, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT (organization_id, id) DO UPDATE SET
               source_connection_id = excluded.source_connection_id,
               target_connection_id = excluded.target_connection_id,
               status = excluded.status,
               payload_json = excluded.payload_json,
               created_at = excluded.created_at,
               updated_at = excluded.updated_at",
            params![
                scope.organization_id.as_str(),
                normalized.id,
                normalized.source_connection_id,
                normalized.target_connection_id,
                normalized.status,
                payload,
                normalized.created_at.to_rfc3339(),
                normalized.updated_at.to_rfc3339(),
            ],
        )
        .map_err(unavailable)?;
        Ok(())
    }

    fn delete_sync(
        conn: &Connection,
        scope: &OrganizationScope,
        task_id: &str,
    ) -> Result<(), SyncAssetStoreError> {
        conn.execute(
            "DELETE FROM sync_tasks WHERE organization_id = ?1 AND id = ?2",
            params![scope.organization_id.as_str(), task_id],
        )
        .map_err(unavailable)?;
        Ok(())
    }

    /// 全量任务列表（桌面 `get_sync_tasks` 的服务端对应物；桌面按文件
    /// 插入序，服务端按创建时刻稳定序）。
    pub async fn list(
        &self,
        scope: &OrganizationScope,
    ) -> Result<Vec<StoredSyncTask>, SyncAssetStoreError> {
        let store = self.store.clone();
        let scope = scope.clone();
        tokio::task::spawn_blocking(move || {
            let conn = store.lock_for_adapter().map_err(unavailable)?;
            Self::list_sync(&conn, &scope)
        })
        .await
        .map_err(|error| unavailable(format!("sync task list task: {error}")))?
    }

    /// 保存/覆盖一个任务（桌面 `save_sync_task_direct` 的存储半边；校验
    /// 与连接在场性核对在域层，存储只落行）。
    pub async fn save(
        &self,
        scope: &OrganizationScope,
        task: StoredSyncTask,
    ) -> Result<(), SyncAssetStoreError> {
        let store = self.store.clone();
        let scope = scope.clone();
        tokio::task::spawn_blocking(move || {
            let conn = store.lock_for_adapter().map_err(unavailable)?;
            Self::save_sync(&conn, &scope, &task)
        })
        .await
        .map_err(|error| unavailable(format!("sync task save task: {error}")))?
    }

    /// 删除任务（桌面 `delete_sync_task` 的服务端对应物）。
    pub async fn delete(
        &self,
        scope: &OrganizationScope,
        task_id: &str,
    ) -> Result<(), SyncAssetStoreError> {
        let store = self.store.clone();
        let scope = scope.clone();
        let task_id = task_id.to_owned();
        tokio::task::spawn_blocking(move || {
            let conn = store.lock_for_adapter().map_err(unavailable)?;
            Self::delete_sync(&conn, &scope, &task_id)
        })
        .await
        .map_err(|error| unavailable(format!("sync task delete task: {error}")))?
    }
}

/// v15 `sync_profiles` 表的行存取（可复用同步配置）。
#[derive(Clone)]
pub struct ServerSyncProfiles {
    store: Arc<AdminStore>,
}

impl ServerSyncProfiles {
    pub fn new(store: Arc<AdminStore>) -> Self {
        Self { store }
    }

    fn list_sync(
        conn: &Connection,
        scope: &OrganizationScope,
    ) -> Result<Vec<SyncProfile>, SyncAssetStoreError> {
        let mut stmt = conn
            .prepare(
                "SELECT id, version, name, source_connection_id, target_connection_id, \
                 source_database, target_database, source_schema, target_schema, \
                 tables_json, options_json, created_at, updated_at \
                 FROM sync_profiles WHERE organization_id = ?1 ORDER BY name, id",
            )
            .map_err(unavailable)?;
        let rows = stmt
            .query_map(params![scope.organization_id.as_str()], profile_from_row)
            .map_err(unavailable)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(unavailable)?;
        Ok(rows)
    }

    fn save_sync(
        conn: &Connection,
        scope: &OrganizationScope,
        profile: &SyncProfile,
    ) -> Result<(), SyncAssetStoreError> {
        conn.execute(
            "INSERT INTO sync_profiles (
               organization_id, id, version, name,
               source_connection_id, target_connection_id,
               source_database, target_database, source_schema, target_schema,
               tables_json, options_json, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
             ON CONFLICT (organization_id, id) DO UPDATE SET
               version = excluded.version, name = excluded.name,
               source_connection_id = excluded.source_connection_id,
               target_connection_id = excluded.target_connection_id,
               source_database = excluded.source_database,
               target_database = excluded.target_database,
               source_schema = excluded.source_schema,
               target_schema = excluded.target_schema,
               tables_json = excluded.tables_json,
               options_json = excluded.options_json,
               created_at = excluded.created_at,
               updated_at = excluded.updated_at",
            params![
                scope.organization_id.as_str(),
                profile.id,
                profile.version as i64,
                profile.name,
                profile.source_connection_id,
                profile.target_connection_id,
                profile.source_database,
                profile.target_database,
                profile.source_schema,
                profile.target_schema,
                serde_json::to_string(&profile.tables).map_err(unavailable)?,
                serde_json::to_string(&profile.options).map_err(unavailable)?,
                profile.created_at.to_rfc3339(),
                profile.updated_at.to_rfc3339(),
            ],
        )
        .map_err(unavailable)?;
        Ok(())
    }

    fn delete_sync(
        conn: &Connection,
        scope: &OrganizationScope,
        profile_id: &str,
    ) -> Result<(), SyncAssetStoreError> {
        conn.execute(
            "DELETE FROM sync_profiles WHERE organization_id = ?1 AND id = ?2",
            params![scope.organization_id.as_str(), profile_id],
        )
        .map_err(unavailable)?;
        Ok(())
    }

    /// 全量档案列表（桌面 `get_sync_profiles` 的服务端对应物；按名称
    /// 稳定序）。
    pub async fn list(
        &self,
        scope: &OrganizationScope,
    ) -> Result<Vec<SyncProfile>, SyncAssetStoreError> {
        let store = self.store.clone();
        let scope = scope.clone();
        tokio::task::spawn_blocking(move || {
            let conn = store.lock_for_adapter().map_err(unavailable)?;
            Self::list_sync(&conn, &scope)
        })
        .await
        .map_err(|error| unavailable(format!("sync profile list task: {error}")))?
    }

    /// 保存/覆盖一个档案（桌面 `save_sync_profile` 的存储半边；校验、
    /// 连接在场性与 `updated_at` 推进在域层，存储只落行）。
    pub async fn save(
        &self,
        scope: &OrganizationScope,
        profile: SyncProfile,
    ) -> Result<(), SyncAssetStoreError> {
        let store = self.store.clone();
        let scope = scope.clone();
        tokio::task::spawn_blocking(move || {
            let conn = store.lock_for_adapter().map_err(unavailable)?;
            Self::save_sync(&conn, &scope, &profile)
        })
        .await
        .map_err(|error| unavailable(format!("sync profile save task: {error}")))?
    }

    /// 删除档案（桌面 `delete_sync_profile` 的服务端对应物）。
    pub async fn delete(
        &self,
        scope: &OrganizationScope,
        profile_id: &str,
    ) -> Result<(), SyncAssetStoreError> {
        let store = self.store.clone();
        let scope = scope.clone();
        let profile_id = profile_id.to_owned();
        tokio::task::spawn_blocking(move || {
            let conn = store.lock_for_adapter().map_err(unavailable)?;
            Self::delete_sync(&conn, &scope, &profile_id)
        })
        .await
        .map_err(|error| unavailable(format!("sync profile delete task: {error}")))?
    }
}

fn profile_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SyncProfile> {
    let id: String = row.get(0)?;
    let version: i64 = row.get(1)?;
    let name: String = row.get(2)?;
    let source_connection_id: String = row.get(3)?;
    let target_connection_id: String = row.get(4)?;
    let source_database: Option<String> = row.get(5)?;
    let target_database: Option<String> = row.get(6)?;
    let source_schema: Option<String> = row.get(7)?;
    let target_schema: Option<String> = row.get(8)?;
    let tables_json: String = row.get(9)?;
    let options_json: String = row.get(10)?;
    let created_at: String = row.get(11)?;
    let updated_at: String = row.get(12)?;

    let invalid = |column: usize, what: &str| {
        rusqlite::Error::FromSqlConversionFailure(
            column,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                what.to_string(),
            )),
        )
    };
    let created_at = DateTime::parse_from_rfc3339(&created_at)
        .map_err(|_| invalid(11, "profile created_at"))?
        .with_timezone(&Utc);
    let updated_at = DateTime::parse_from_rfc3339(&updated_at)
        .map_err(|_| invalid(12, "profile updated_at"))?
        .with_timezone(&Utc);
    Ok(SyncProfile {
        version: u32::try_from(version).map_err(|_| invalid(1, "profile version"))?,
        id,
        name,
        source_connection_id,
        target_connection_id,
        source_database,
        target_database,
        source_schema,
        target_schema,
        tables: serde_json::from_str(&tables_json).map_err(|_| invalid(9, "profile tables"))?,
        options: serde_json::from_str(&options_json).map_err(|_| invalid(10, "profile options"))?,
        created_at,
        updated_at,
    })
}
