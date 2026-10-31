//! `ProfileRepository` 的服务端适配器（WR-D2 第一片）。
//!
//! 同一 domain trait、另一个宿主的实现：语义与桌面 `DesktopProfiles`
//! **逐方法对齐**（同一用例、两种绑定）——CM-05 越组织不可见、CAS 只承诺
//! `CasConflict`、create 幂等重放返回既有行、disable 只翻 `enabled` 并递增
//! `config_revision`、`advance_credential_revision` 只动凭据版本。差异只在
//! 存储（管理库 vs 本地库）与 `secret_ref` 的铸造方式（`server-credential:`
//! 前缀，解析归 WR-D4 的 `SecretProvider`）。
//!
//! 异步边界：内核 trait 是 async，SQLite 是同步——每个 trait 方法把同步
//! 核心丢进 `spawn_blocking`，差异只允许出现在这一层。

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use rusqlite::{params, OptionalExtension, Row};

use datazen::connection::domain::{
    NamespaceTarget, ProfileRecord, ProfileReader, ProfileRepository, ProfileScope,
};
use datazen::connection::dto::{ProfileDraft, ProfilePatch};
use datazen::shared::context::RequestContext;
use datazen::shared::error::PortError;
use datazen::shared::id::{
    ConfigRevision, ConnectionId, CredentialRevision, IdempotencyKey, NetworkRouteRef,
    NetworkRouteRevision, OrganizationId, SecretRef, Timestamp,
};

use super::AdminStore;

/// 敏感词标记：出现在 `public_options` 键里即拒绝（与桌面 `DesktopProfiles`
/// 同一份词表——公开选项不得夹带凭据，凭据只走 `credentials` → `SecretRef`）。
const SENSITIVE_OPTION_MARKERS: [&str; 8] = [
    "password",
    "secret",
    "token",
    "credential",
    "privatekey",
    "private_key",
    "apikey",
    "api_key",
];

/// 行投影的完整列清单（所有 SELECT 共用，顺序即 `record_from_row` 的下标）。
const PROFILE_COLUMNS: &str = "organization_id, connection_id, name, driver_id, config_revision, \
     credential_revision, network_route_revision, initial_namespace_json, driver_options_json, \
     secret_ref, network_route_ref, read_only, policy_ref, enabled, created_at, updated_at";

#[derive(Clone)]
pub struct ServerProfiles {
    store: Arc<AdminStore>,
}

impl ServerProfiles {
    /// 在已迁移的管理库上构造。调用方负责先跑 `AdminStore::migrate`。
    pub fn new(store: Arc<AdminStore>) -> Self {
        Self { store }
    }

    fn backend(source: impl std::fmt::Display) -> PortError {
        PortError::BackendUnavailable(source.to_string())
    }

    fn reject_sensitive_options(
        options: &BTreeMap<String, serde_json::Value>,
    ) -> Result<(), PortError> {
        if options.keys().any(|key| {
            SENSITIVE_OPTION_MARKERS
                .iter()
                .any(|marker| key.to_ascii_lowercase().contains(marker))
        }) {
            return Err(PortError::BackendUnavailable(
                "sensitive options must use credentials".into(),
            ));
        }
        Ok(())
    }

    fn now() -> String {
        chrono::Utc::now().to_rfc3339()
    }

    fn next_revision(value: u64) -> Result<u64, PortError> {
        value
            .checked_add(1)
            .ok_or_else(|| PortError::BackendUnavailable("revision exhausted".into()))
    }

    // ── 同步核心（trait 实现只做 spawn_blocking 包装） ──

    fn sync_list(&self, organization_id: &str) -> Result<Vec<ProfileRecord>, PortError> {
        let conn = lock(&self.store)?;
        let mut stmt = conn
            .prepare(&format!(
                "SELECT {PROFILE_COLUMNS} FROM connection_profile
                 WHERE organization_id = ?1 AND enabled = 1
                 ORDER BY name, connection_id"
            ))
            .map_err(Self::backend)?;
        let rows = stmt
            .query_map([organization_id], record_from_row)
            .map_err(Self::backend)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Self::backend)?;
        Ok(rows)
    }

    fn sync_get(
        &self,
        organization_id: &str,
        id: &str,
    ) -> Result<Option<ProfileRecord>, PortError> {
        let conn = lock(&self.store)?;
        conn.query_row(
            &format!(
                "SELECT {PROFILE_COLUMNS} FROM connection_profile
                 WHERE organization_id = ?1 AND connection_id = ?2"
            ),
            params![organization_id, id],
            record_from_row,
        )
        .optional()
        .map_err(Self::backend)
    }

    fn sync_create(
        &self,
        organization_id: &str,
        principal_id: &str,
        draft: ProfileDraft,
        idem: &IdempotencyKey,
    ) -> Result<ProfileRecord, PortError> {
        Self::reject_sensitive_options(&draft.public_options)?;
        let creation_key = format!("{principal_id}:{}", idem.as_str());

        let conn = lock(&self.store)?;

        // 幂等重放：同键返回既有行（与桌面 platform_creation 同语义）。
        let replayed: Option<String> = conn
            .query_row(
                "SELECT connection_id FROM connection_profile
                 WHERE organization_id = ?1 AND creation_key = ?2",
                params![organization_id, creation_key],
                |row| row.get(0),
            )
            .optional()
            .map_err(Self::backend)?;
        if let Some(existing) = replayed {
            return conn
                .query_row(
                    &format!(
                        "SELECT {PROFILE_COLUMNS} FROM connection_profile
                         WHERE organization_id = ?1 AND connection_id = ?2"
                    ),
                    params![organization_id, existing],
                    record_from_row,
                )
                .map_err(Self::backend);
        }

        let connection_id = uuid::Uuid::new_v4().to_string();
        let at = Self::now();
        let namespace_json = serde_json::to_string(&draft.initial_namespace)
            .map_err(|_| PortError::BackendUnavailable("namespace serialization failed".into()))?;
        let options_json = serde_json::to_string(&draft.public_options)
            .map_err(|_| PortError::BackendUnavailable("options serialization failed".into()))?;
        // 凭据明文不落库：行里只有 SecretRef 引用（解析归 WR-D4）。
        let secret_ref = if draft.has_credentials() {
            Some(format!("server-credential:{connection_id}:1"))
        } else {
            None
        };

        conn.execute(
            "INSERT INTO connection_profile (
               organization_id, connection_id, name, driver_id, config_revision,
               credential_revision, network_route_revision, initial_namespace_json,
               driver_options_json, secret_ref, network_route_ref, read_only,
               policy_ref, enabled, created_at, updated_at, creation_key
             ) VALUES (?1, ?2, ?3, ?4, 1, 1, 1, ?5, ?6, ?7, NULL, 0, NULL, 1, ?8, ?8, ?9)",
            params![
                organization_id,
                connection_id,
                draft.name,
                draft.driver_id,
                namespace_json,
                options_json,
                secret_ref,
                at,
                creation_key,
            ],
        )
        .map_err(Self::backend)?;

        conn.query_row(
            &format!(
                "SELECT {PROFILE_COLUMNS} FROM connection_profile
                 WHERE organization_id = ?1 AND connection_id = ?2"
            ),
            params![organization_id, connection_id],
            record_from_row,
        )
        .map_err(Self::backend)
    }

    fn sync_compare_and_set(
        &self,
        organization_id: &str,
        id: &str,
        expected: u64,
        patch: ProfilePatch,
    ) -> Result<ProfileRecord, PortError> {
        if let Some(options) = &patch.public_options {
            Self::reject_sensitive_options(options)?;
        }
        let touches_credentials = patch.touches_credentials();

        let conn = lock(&self.store)?;

        let existing = conn
            .query_row(
                "SELECT config_revision, credential_revision, initial_namespace_json,
                        driver_options_json, secret_ref
                 FROM connection_profile
                 WHERE organization_id = ?1 AND connection_id = ?2",
                params![organization_id, id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, Option<String>>(4)?,
                    ))
                },
            )
            .optional()
            .map_err(Self::backend)?
            .ok_or_else(|| PortError::NotFound("profile".into()))?;

        let (config_revision, credential_revision, namespace_json, options_json, secret_ref) =
            existing;
        if config_revision as u64 != expected {
            return Err(PortError::CasConflict {
                entity: "profile",
                id: id.to_string(),
            });
        }

        let name_clause = patch.name.as_deref();
        let new_namespace_json = match &patch.initial_namespace {
            Some(namespace) => serde_json::to_string(namespace).map_err(|_| {
                PortError::BackendUnavailable("namespace serialization failed".into())
            })?,
            None => namespace_json,
        };
        let new_options_json = match &patch.public_options {
            Some(options) => serde_json::to_string(options).map_err(|_| {
                PortError::BackendUnavailable("options serialization failed".into())
            })?,
            None => options_json,
        };
        // 凭据补丁只铸造新的 SecretRef 并递增凭据版本；明文不落库。
        let new_credential_revision = if touches_credentials {
            Self::next_revision(credential_revision as u64)?
        } else {
            credential_revision as u64
        };
        let new_secret_ref = if touches_credentials {
            Some(format!("server-credential:{id}:{new_credential_revision}"))
        } else {
            secret_ref
        };
        let new_config_revision = Self::next_revision(config_revision as u64)?;
        let at = Self::now();

        let updated = conn
            .execute(
                "UPDATE connection_profile SET
                   name = COALESCE(?3, name),
                   initial_namespace_json = ?4,
                   driver_options_json = ?5,
                   secret_ref = ?6,
                   credential_revision = ?7,
                   config_revision = ?8,
                   updated_at = ?9
                 WHERE organization_id = ?1 AND connection_id = ?2 AND config_revision = ?10",
                params![
                    organization_id,
                    id,
                    name_clause,
                    new_namespace_json,
                    new_options_json,
                    new_secret_ref,
                    new_credential_revision as i64,
                    new_config_revision as i64,
                    at,
                    config_revision,
                ],
            )
            .map_err(Self::backend)?;
        if updated == 0 {
            // 行存在（上面已验证）但版本已变：并发写先到。
            return Err(PortError::CasConflict {
                entity: "profile",
                id: id.to_string(),
            });
        }

        conn.query_row(
            &format!(
                "SELECT {PROFILE_COLUMNS} FROM connection_profile
                 WHERE organization_id = ?1 AND connection_id = ?2"
            ),
            params![organization_id, id],
            record_from_row,
        )
        .map_err(Self::backend)
    }

    fn sync_disable(&self, organization_id: &str, id: &str) -> Result<ProfileRecord, PortError> {
        let conn = lock(&self.store)?;
        let existing = conn
            .query_row(
                "SELECT config_revision FROM connection_profile
                 WHERE organization_id = ?1 AND connection_id = ?2",
                params![organization_id, id],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(Self::backend)?
            .ok_or_else(|| PortError::NotFound("profile".into()))?;
        let next = Self::next_revision(existing as u64)?;
        conn.execute(
            "UPDATE connection_profile
             SET enabled = 0, config_revision = ?3, updated_at = ?4
             WHERE organization_id = ?1 AND connection_id = ?2 AND config_revision = ?5",
            params![organization_id, id, next as i64, Self::now(), existing],
        )
        .map_err(Self::backend)?;
        conn.query_row(
            &format!(
                "SELECT {PROFILE_COLUMNS} FROM connection_profile
                 WHERE organization_id = ?1 AND connection_id = ?2"
            ),
            params![organization_id, id],
            record_from_row,
        )
        .map_err(Self::backend)
    }

    fn sync_advance_credential_revision(
        &self,
        organization_id: &str,
        id: &str,
    ) -> Result<CredentialRevision, PortError> {
        let conn = lock(&self.store)?;
        let existing = conn
            .query_row(
                "SELECT credential_revision FROM connection_profile
                 WHERE organization_id = ?1 AND connection_id = ?2",
                params![organization_id, id],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(Self::backend)?
            .ok_or_else(|| PortError::NotFound("profile".into()))?;
        let next = Self::next_revision(existing as u64)?;
        conn.execute(
            "UPDATE connection_profile
             SET credential_revision = ?3, updated_at = ?4
             WHERE organization_id = ?1 AND connection_id = ?2 AND credential_revision = ?5",
            params![organization_id, id, next as i64, Self::now(), existing],
        )
        .map_err(Self::backend)?;
        Ok(CredentialRevision::new(next))
    }
}

fn lock(
    store: &AdminStore,
) -> Result<std::sync::MutexGuard<'_, rusqlite::Connection>, PortError> {
    store
        .lock_for_adapter()
        .map_err(|source| PortError::BackendUnavailable(source.to_string()))
}

fn record_from_row(row: &Row<'_>) -> rusqlite::Result<ProfileRecord> {
    let organization_id: String = row.get(0)?;
    let connection_id: String = row.get(1)?;
    let name: String = row.get(2)?;
    let driver_id: String = row.get(3)?;
    let config_revision: i64 = row.get(4)?;
    let credential_revision: i64 = row.get(5)?;
    let network_route_revision: i64 = row.get(6)?;
    let namespace_json: String = row.get(7)?;
    let options_json: String = row.get(8)?;
    let secret_ref: Option<String> = row.get(9)?;
    let network_route_ref: Option<String> = row.get(10)?;
    let read_only: i64 = row.get(11)?;
    let policy_ref: Option<String> = row.get(12)?;
    let enabled: i64 = row.get(13)?;
    let created_at: String = row.get(14)?;
    let updated_at: String = row.get(15)?;

    let initial_namespace: NamespaceTarget = serde_json::from_str(&namespace_json).map_err(|_| {
        rusqlite::Error::FromSqlConversionFailure(
            7,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "namespace json",
            )),
        )
    })?;
    let driver_options: BTreeMap<String, serde_json::Value> =
        serde_json::from_str(&options_json).map_err(|_| {
            rusqlite::Error::FromSqlConversionFailure(
                8,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "options json",
                )),
            )
        })?;

    Ok(ProfileRecord {
        organization_id: OrganizationId::new(organization_id),
        connection_id: ConnectionId::new(connection_id),
        name,
        driver_id,
        config_revision: ConfigRevision::new(config_revision as u64),
        initial_namespace,
        driver_options,
        secret_ref: secret_ref.map(SecretRef::new),
        credential_revision: CredentialRevision::new(credential_revision as u64),
        network_route_ref: network_route_ref.map(NetworkRouteRef::new),
        network_route_revision: NetworkRouteRevision::new(network_route_revision as u64),
        read_only: read_only != 0,
        policy_ref,
        enabled: enabled != 0,
        created_at: Timestamp::new(&created_at),
        updated_at: Timestamp::new(&updated_at),
    })
}

#[async_trait]
impl ProfileReader for ServerProfiles {
    async fn get(
        &self,
        ctx: &RequestContext,
        connection_id: ConnectionId,
    ) -> Result<Option<ProfileRecord>, PortError> {
        let organization_id = ctx.organization_id.as_str().to_string();
        let id = connection_id.as_str().to_string();
        let this = self.clone();
        tokio::task::spawn_blocking(move || this.sync_get(&organization_id, &id))
            .await
            .map_err(|source| PortError::BackendUnavailable(source.to_string()))?
    }

    async fn list(
        &self,
        ctx: &RequestContext,
        scope: ProfileScope,
    ) -> Result<Vec<ProfileRecord>, PortError> {
        // CM-05：作用域越过请求自身的组织 → 空，不报错、不泄漏。
        if scope.organization_id != ctx.organization_id {
            return Ok(vec![]);
        }
        let organization_id = ctx.organization_id.as_str().to_string();
        let this = self.clone();
        tokio::task::spawn_blocking(move || this.sync_list(&organization_id))
            .await
            .map_err(|source| PortError::BackendUnavailable(source.to_string()))?
    }
}

#[async_trait]
impl ProfileRepository for ServerProfiles {
    async fn list(
        &self,
        ctx: &RequestContext,
        scope: ProfileScope,
    ) -> Result<Vec<ProfileRecord>, PortError> {
        <Self as ProfileReader>::list(self, ctx, scope).await
    }

    async fn get(
        &self,
        ctx: &RequestContext,
        connection_id: ConnectionId,
    ) -> Result<Option<ProfileRecord>, PortError> {
        <Self as ProfileReader>::get(self, ctx, connection_id).await
    }

    async fn create(
        &self,
        ctx: &RequestContext,
        draft: ProfileDraft,
        idem: &IdempotencyKey,
    ) -> Result<ProfileRecord, PortError> {
        let organization_id = ctx.organization_id.as_str().to_string();
        let principal_id = ctx.principal_id.as_str().to_string();
        let idem = idem.clone();
        let this = self.clone();
        tokio::task::spawn_blocking(move || {
            this.sync_create(&organization_id, &principal_id, draft, &idem)
        })
        .await
        .map_err(|source| PortError::BackendUnavailable(source.to_string()))?
    }

    async fn compare_and_set(
        &self,
        ctx: &RequestContext,
        connection_id: ConnectionId,
        expected: ConfigRevision,
        patch: ProfilePatch,
    ) -> Result<ProfileRecord, PortError> {
        let organization_id = ctx.organization_id.as_str().to_string();
        let id = connection_id.as_str().to_string();
        let expected = expected.counter().get();
        let this = self.clone();
        tokio::task::spawn_blocking(move || {
            this.sync_compare_and_set(&organization_id, &id, expected, patch)
        })
        .await
        .map_err(|source| PortError::BackendUnavailable(source.to_string()))?
    }

    async fn disable(
        &self,
        ctx: &RequestContext,
        connection_id: ConnectionId,
    ) -> Result<ProfileRecord, PortError> {
        let organization_id = ctx.organization_id.as_str().to_string();
        let id = connection_id.as_str().to_string();
        let this = self.clone();
        tokio::task::spawn_blocking(move || this.sync_disable(&organization_id, &id))
            .await
            .map_err(|source| PortError::BackendUnavailable(source.to_string()))?
    }

    async fn advance_credential_revision(
        &self,
        ctx: &RequestContext,
        connection_id: ConnectionId,
    ) -> Result<CredentialRevision, PortError> {
        let organization_id = ctx.organization_id.as_str().to_string();
        let id = connection_id.as_str().to_string();
        let this = self.clone();
        tokio::task::spawn_blocking(move || {
            this.sync_advance_credential_revision(&organization_id, &id)
        })
        .await
        .map_err(|source| PortError::BackendUnavailable(source.to_string()))?
    }
}
