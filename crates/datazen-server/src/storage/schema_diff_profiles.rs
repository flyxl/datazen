//! Schema Diff profile 的服务端 SQLite 适配器（W8 刀3b；v12
//! `schema_diff_profiles` 表）。
//!
//! 词汇（[`SchemaDiffProfile`]）与持久化端口
//! （[`SchemaDiffProfileRepository`]）唯一定义在共享层
//! `datazen::migration::schema_diff`（桌面实现是加密 JSON 文件——单租户
//! 全体共享；web 形态改存服务端库行，查询资产 v11 的「文件形的服务端
//! 对偶」同款裁决）。本件只负责行存取与隔离边界：
//!
//! - **per organization 隔离**：profile 是「连接对 + 表选择」级别的组织
//!   资产（桌面单租户不区分归属；服务端按组织收敛，CM-05：越界读空、
//!   写只落本边界）。隔离键来自 [`OrganizationScope`]——由调用方按认证
//!   链事实装配（INV-01：组织不来自请求体）。
//! - **同步核心 + `spawn_blocking` 包装**（settings / query_assets 同款
//!   异步边界）。
//! - **集合列 JSON 投影**：`tables_json` / `source_objects_json` 等
//!   （v1 connection_profile 的投影列口径）；行读取时反序列化回共享层
//!   类型，坏行按存储故障上抛（不静默丢弃——桌面加密文件形同款纪律：
//!   `load_…_from_disk` 对坏行是 validate 过滤，服务端选择更严的
//!   fail-closed，因为服务端行只经本适配器写入，坏行意味着存储损坏）。

use std::sync::Arc;

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};

use datazen::migration::schema_diff::profile::SchemaDiffProfile;
use datazen::migration::schema_diff::SchemaDiffProfileRepository;
use datazen::migration::schema_diff::SchemaDiffProfileRepositoryError;
use datazen::shared::scope::OrganizationScope;

use crate::storage::AdminStore;

fn unavailable(detail: impl std::fmt::Display) -> SchemaDiffProfileRepositoryError {
    SchemaDiffProfileRepositoryError::Unavailable(detail.to_string())
}

/// v12 表的行存取（`SchemaDiffProfileRepository` 的服务端实现）。
#[derive(Clone)]
pub struct ServerSchemaDiffProfiles {
    store: Arc<AdminStore>,
}

impl ServerSchemaDiffProfiles {
    pub fn new(store: Arc<AdminStore>) -> Self {
        Self { store }
    }

    fn list_sync(
        conn: &Connection,
        scope: &OrganizationScope,
    ) -> Result<Vec<SchemaDiffProfile>, SchemaDiffProfileRepositoryError> {
        let mut stmt = conn
            .prepare(
                "SELECT id, version, name, source_connection_id, target_connection_id, \
                 source_database, target_database, source_schema, target_schema, \
                 target_only_tables_json, tables_json, source_objects_json, target_objects_json, \
                 allow_destructive, include_indexes, require_rollback, type_overrides_json, \
                 created_at, updated_at \
                 FROM schema_diff_profiles WHERE organization_id = ?1 ORDER BY name, id",
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
        profile: &SchemaDiffProfile,
    ) -> Result<(), SchemaDiffProfileRepositoryError> {
        conn.execute(
            "INSERT INTO schema_diff_profiles (
               organization_id, id, version, name,
               source_connection_id, target_connection_id,
               source_database, target_database, source_schema, target_schema,
               target_only_tables_json, tables_json,
               source_objects_json, target_objects_json,
               allow_destructive, include_indexes, require_rollback, type_overrides_json,
               created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)
             ON CONFLICT (organization_id, id) DO UPDATE SET
               version = excluded.version, name = excluded.name,
               source_connection_id = excluded.source_connection_id,
               target_connection_id = excluded.target_connection_id,
               source_database = excluded.source_database,
               target_database = excluded.target_database,
               source_schema = excluded.source_schema,
               target_schema = excluded.target_schema,
               target_only_tables_json = excluded.target_only_tables_json,
               tables_json = excluded.tables_json,
               source_objects_json = excluded.source_objects_json,
               target_objects_json = excluded.target_objects_json,
               allow_destructive = excluded.allow_destructive,
               include_indexes = excluded.include_indexes,
               require_rollback = excluded.require_rollback,
               type_overrides_json = excluded.type_overrides_json,
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
                serde_json::to_string(&profile.target_only_tables).map_err(unavailable)?,
                serde_json::to_string(&profile.tables).map_err(unavailable)?,
                serde_json::to_string(&profile.source_objects).map_err(unavailable)?,
                serde_json::to_string(&profile.target_objects).map_err(unavailable)?,
                profile.allow_destructive as i64,
                profile.include_indexes as i64,
                profile.require_rollback as i64,
                serde_json::to_string(&profile.type_overrides).map_err(unavailable)?,
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
    ) -> Result<(), SchemaDiffProfileRepositoryError> {
        conn.execute(
            "DELETE FROM schema_diff_profiles WHERE organization_id = ?1 AND id = ?2",
            params![scope.organization_id.as_str(), profile_id],
        )
        .map_err(unavailable)?;
        Ok(())
    }
}

fn profile_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SchemaDiffProfile> {
    let id: String = row.get(0)?;
    let version: i64 = row.get(1)?;
    let name: String = row.get(2)?;
    let source_connection_id: String = row.get(3)?;
    let target_connection_id: String = row.get(4)?;
    let source_database: String = row.get(5)?;
    let target_database: String = row.get(6)?;
    let source_schema: Option<String> = row.get(7)?;
    let target_schema: Option<String> = row.get(8)?;
    let target_only_tables_json: String = row.get(9)?;
    let tables_json: String = row.get(10)?;
    let source_objects_json: String = row.get(11)?;
    let target_objects_json: String = row.get(12)?;
    let allow_destructive: i64 = row.get(13)?;
    let include_indexes: i64 = row.get(14)?;
    let require_rollback: i64 = row.get(15)?;
    let type_overrides_json: String = row.get(16)?;
    let created_at: String = row.get(17)?;
    let updated_at: String = row.get(18)?;

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
    let decode_collection = |column: usize, what: &str, json: &str| -> rusqlite::Result<String> {
        if json.trim().is_empty() {
            return Ok("[]".to_string());
        }
        let _ =
            serde_json::from_str::<serde_json::Value>(json).map_err(|_| invalid(column, what))?;
        Ok(json.to_string())
    };
    let created_at = DateTime::parse_from_rfc3339(&created_at)
        .map_err(|_| invalid(17, "profile created_at"))?
        .with_timezone(&Utc);
    let updated_at = DateTime::parse_from_rfc3339(&updated_at)
        .map_err(|_| invalid(18, "profile updated_at"))?
        .with_timezone(&Utc);
    Ok(SchemaDiffProfile {
        version: u32::try_from(version).map_err(|_| invalid(1, "profile version"))?,
        id,
        name,
        source_connection_id,
        target_connection_id,
        source_database,
        target_database,
        source_schema,
        target_schema,
        target_only_tables: serde_json::from_str(&decode_collection(
            9,
            "profile target_only_tables",
            &target_only_tables_json,
        )?)
        .map_err(|_| invalid(9, "profile target_only_tables"))?,
        tables: serde_json::from_str(&decode_collection(10, "profile tables", &tables_json)?)
            .map_err(|_| invalid(10, "profile tables"))?,
        source_objects: serde_json::from_str(&decode_collection(
            11,
            "profile source_objects",
            &source_objects_json,
        )?)
        .map_err(|_| invalid(11, "profile source_objects"))?,
        target_objects: serde_json::from_str(&decode_collection(
            12,
            "profile target_objects",
            &target_objects_json,
        )?)
        .map_err(|_| invalid(12, "profile target_objects"))?,
        allow_destructive: allow_destructive != 0,
        include_indexes: include_indexes != 0,
        require_rollback: require_rollback != 0,
        type_overrides: serde_json::from_str(&decode_collection(
            16,
            "profile type_overrides",
            &type_overrides_json,
        )?)
        .map_err(|_| invalid(16, "profile type_overrides"))?,
        created_at,
        updated_at,
    })
}

#[async_trait::async_trait]
impl SchemaDiffProfileRepository for ServerSchemaDiffProfiles {
    async fn list(
        &self,
        scope: &OrganizationScope,
    ) -> Result<Vec<SchemaDiffProfile>, SchemaDiffProfileRepositoryError> {
        let store = self.store.clone();
        let scope = scope.clone();
        tokio::task::spawn_blocking(move || {
            let conn = store.lock_for_adapter().map_err(unavailable)?;
            Self::list_sync(&conn, &scope)
        })
        .await
        .map_err(|error| unavailable(format!("profile list task: {error}")))?
    }

    async fn save(
        &self,
        scope: &OrganizationScope,
        profile: SchemaDiffProfile,
    ) -> Result<(), SchemaDiffProfileRepositoryError> {
        let store = self.store.clone();
        let scope = scope.clone();
        tokio::task::spawn_blocking(move || {
            let conn = store.lock_for_adapter().map_err(unavailable)?;
            Self::save_sync(&conn, &scope, &profile)
        })
        .await
        .map_err(|error| unavailable(format!("profile save task: {error}")))?
    }

    async fn delete(
        &self,
        scope: &OrganizationScope,
        profile_id: &str,
    ) -> Result<(), SchemaDiffProfileRepositoryError> {
        let store = self.store.clone();
        let scope = scope.clone();
        let profile_id = profile_id.to_owned();
        tokio::task::spawn_blocking(move || {
            let conn = store.lock_for_adapter().map_err(unavailable)?;
            Self::delete_sync(&conn, &scope, &profile_id)
        })
        .await
        .map_err(|error| unavailable(format!("profile delete task: {error}")))?
    }
}

/// 桌面 `save` 前的连接存在性校验所需读取：按组织 + 连接 id 取一行
/// `connection_catalog` 的 `connection_id`（在场性探针，不读配置体）。
/// miss / 越组织 → `Ok(false)`（调用方按 Validation 拒绝）。
pub async fn connection_exists(
    store: &Arc<AdminStore>,
    scope: &OrganizationScope,
    connection_id: &str,
) -> Result<bool, SchemaDiffProfileRepositoryError> {
    let store = store.clone();
    let organization_id = scope.organization_id.as_str().to_owned();
    let connection_id = connection_id.to_owned();
    tokio::task::spawn_blocking(move || {
        let conn = store.lock_for_adapter().map_err(unavailable)?;
        Ok(conn
            .query_row(
                "SELECT 1 FROM connection_catalog WHERE organization_id = ?1 AND connection_id = ?2",
                params![organization_id, connection_id],
                |_| Ok(()),
            )
            .optional()
            .map_err(unavailable)?
            .is_some())
    })
    .await
    .map_err(|error| unavailable(format!("profile connection probe task: {error}")))?
}
