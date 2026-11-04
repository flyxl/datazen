//! `TunnelRepository`（独立持久化隧道定义）的服务端适配器（WR-D2 收尾）。
//!
//! 端口（`datazen::tunnel::domain::tunnel_repository`）的形状是
//! `datazen-driver-api` 的 [`SavedTunnel`] / [`SavedTunnelSummary`] /
//! [`TunnelUsage`]。存储形状（v7 `tunnel_definition` + 复用 `stored_secret`）：
//!
//! - `kind` 列的 CHECK 取值即 driver-api `TunnelKind` 的三个持久化字面量
//!   （`none` 不是持久化形态——SavedTunnel 契约排除它）。
//! - **凭据永不明文落库**：`ssh.password` / `ssh.passphrase` /
//!   `http_proxy.auth_token` / `websocket.auth_token` 经
//!   [`ServerSecretVault`] 密封（`secret_ref = server-vault:tunnel:{org}:{id}`）；
//!   名单档 [`SavedTunnelSummary`] 天然无秘密字段（端口语义钉）。
//! - `usage` 反向投影走 `connection_catalog.tunnel_id` 投影列的索引（一次
//!   索引查询，不扫 JSON——迁移文档登记的裁决）。
//! - 组织隔离沿 CM-05：读越界 = 空/None；写越界无面（scope 即组织，
//!   save 在本组织内 upsert；delete 越界 = `NotFound` 经 `PortError`）。

use std::sync::Arc;

use async_trait::async_trait;
use datazen_driver_api::{SavedTunnel, SavedTunnelSummary, TunnelKind, TunnelUsage};
use rusqlite::{params, OptionalExtension};

use datazen::shared::error::PortError;
use datazen::shared::scope::OrganizationScope;
use datazen::tunnel::domain::TunnelRepository;

use crate::execution::secret_provider::{
    extract_sensitive_fields, splice_sensitive_fields, ServerSecretVault,
};
use crate::storage::AdminStore;

fn now_string() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn store_backend(error: crate::storage::StorageError) -> PortError {
    PortError::BackendUnavailable(error.to_string())
}

fn backend(error: rusqlite::Error) -> PortError {
    PortError::BackendUnavailable(error.to_string())
}

fn kind_literal(kind: TunnelKind) -> Result<&'static str, PortError> {
    match kind {
        TunnelKind::Ssh => Ok("ssh"),
        TunnelKind::HttpProxy => Ok("httpProxy"),
        TunnelKind::WebSocket => Ok("websocket"),
        TunnelKind::None => Err(PortError::BackendUnavailable(
            "tunnel kind `none` is not a persisted shape".into(),
        )),
    }
}

fn kind_of(literal: &str) -> Result<TunnelKind, PortError> {
    match literal {
        "ssh" => Ok(TunnelKind::Ssh),
        "httpProxy" => Ok(TunnelKind::HttpProxy),
        "websocket" => Ok(TunnelKind::WebSocket),
        other => Err(PortError::BackendUnavailable(format!(
            "tunnel kind {other} is outside the persisted vocabulary"
        ))),
    }
}

fn ref_of(secret_ref: &str) -> datazen::shared::id::SecretRef {
    datazen::shared::id::SecretRef::new(secret_ref.to_string())
}

/// 隧道定义的服务端适配器。每组织一个实例（scope 参数即组织边界）。
#[derive(Clone)]
pub struct ServerTunnels {
    store: Arc<AdminStore>,
    vault: Arc<ServerSecretVault>,
}

impl std::fmt::Debug for ServerTunnels {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerTunnels")
            .field("store", &"AdminStore")
            .field("vault", &"ServerSecretVault")
            .finish()
    }
}

impl ServerTunnels {
    pub fn new(store: Arc<AdminStore>, vault: Arc<ServerSecretVault>) -> Self {
        Self { store, vault }
    }

    // ── 同步核心（trait 实现只做 spawn_blocking 包装） ──

    /// 存储行 → `SavedTunnel`：净化投影拼回敏感子树。
    fn tunnel_from_parts(
        &self,
        organization_id: &str,
        tunnel_id: &str,
        _name: String,
        _kind: TunnelKind,
        secret_ref: Option<String>,
        config_json: String,
    ) -> Result<SavedTunnel, PortError> {
        let mut value: serde_json::Value = serde_json::from_str(&config_json)
            .map_err(|error| backend(rusqlite::Error::ToSqlConversionFailure(error.into())))?;
        if let Some(secret_ref) = secret_ref {
            let referenced = ref_of(&secret_ref);
            let revision = self
                .vault
                .sync_current_revision(organization_id, &referenced)
                .map_err(|error| error)?
                .ok_or_else(|| {
                    PortError::BackendUnavailable(format!(
                        "secret material missing for {tunnel_id}"
                    ))
                })?;
            let material = self
                .vault
                .sync_open_revision(organization_id, &referenced, revision)
                .map_err(|error| error)?
                .ok_or_else(|| {
                    PortError::BackendUnavailable(format!(
                        "secret revision missing for {tunnel_id}"
                    ))
                })?;
            let secrets: serde_json::Value = serde_json::from_slice(&material)
                .map_err(|error| backend(rusqlite::Error::ToSqlConversionFailure(error.into())))?;
            splice_sensitive_fields(&mut value, &secrets);
        }
        serde_json::from_value(value)
            .map_err(|error| backend(rusqlite::Error::ToSqlConversionFailure(error.into())))
    }

    fn sync_get(
        &self,
        organization_id: &str,
        tunnel_id: &str,
    ) -> Result<Option<SavedTunnel>, PortError> {
        // 读原始行在先（锁内），解封拼回在后（锁外）——vault 的 sync_* 会
        // 锁同一把 AdminStore 连接，锁内调用必然自锁死。
        let row = {
            let conn = self.store.lock_for_adapter().map_err(store_backend)?;
            conn.query_row(
                "SELECT name, kind, secret_ref, config_json FROM tunnel_definition
                 WHERE organization_id = ?1 AND tunnel_id = ?2",
                params![organization_id, tunnel_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .optional()
            .map_err(backend)?
        };
        let Some((name, kind_literal, secret_ref, config_json)) = row else {
            return Ok(None);
        };
        let tunnel = self.tunnel_from_parts(
            organization_id,
            tunnel_id,
            name,
            kind_of(&kind_literal)?,
            secret_ref,
            config_json,
        )?;
        Ok(Some(tunnel))
    }

    fn sync_list(&self, organization_id: &str) -> Result<Vec<SavedTunnelSummary>, PortError> {
        let conn = self.store.lock_for_adapter().map_err(store_backend)?;
        let mut stmt = conn
            .prepare(
                "SELECT tunnel_id, name, kind FROM tunnel_definition
                 WHERE organization_id = ?1 ORDER BY name, tunnel_id",
            )
            .map_err(backend)?;
        let rows = stmt
            .query_map([organization_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(backend)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(backend)?;
        rows.into_iter()
            .map(|(id, name, kind)| {
                Ok(SavedTunnelSummary {
                    id,
                    name,
                    kind: kind_of(&kind)?,
                })
            })
            .collect()
    }

    fn sync_save(&self, organization_id: &str, tunnel: SavedTunnel) -> Result<(), PortError> {
        let kind = kind_literal(tunnel.kind)?;
        let tunnel_id = tunnel.id.clone();
        let mut value = serde_json::to_value(&tunnel).map_err(|error| {
            PortError::BackendUnavailable(format!("tunnel is not serializable: {error}"))
        })?;
        let secrets = extract_sensitive_fields(&mut value);
        let config_json = serde_json::to_string(&value).map_err(|error| {
            PortError::BackendUnavailable(format!(
                "sanitized projection is not serializable: {error}"
            ))
        })?;
        let has_material = secrets
            .as_object()
            .map(|map| !map.is_empty())
            .unwrap_or(false);
        let secret_ref = if has_material {
            let secret_ref = ServerSecretVault::mint_ref("tunnel", organization_id, &tunnel_id);
            let material = serde_json::to_vec(&secrets).map_err(|error| {
                PortError::BackendUnavailable(format!(
                    "sealed material is not serializable: {error}"
                ))
            })?;
            self.vault
                .sync_store_material(organization_id, &secret_ref, &material)
                .map_err(|error| error)?;
            Some(secret_ref.as_str().to_string())
        } else {
            None
        };
        let mut conn = self.store.lock_for_adapter().map_err(store_backend)?;
        let tx = conn.transaction().map_err(backend)?;
        let old_ref: Option<String> = tx
            .query_row(
                "SELECT secret_ref FROM tunnel_definition
                 WHERE organization_id = ?1 AND tunnel_id = ?2",
                params![organization_id, tunnel_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(backend)?;
        let at = now_string();
        tx.execute(
            "INSERT INTO tunnel_definition
               (organization_id, tunnel_id, name, kind, secret_ref, config_json, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)
             ON CONFLICT(organization_id, tunnel_id) DO UPDATE SET
               name = excluded.name,
               kind = excluded.kind,
               secret_ref = excluded.secret_ref,
               config_json = excluded.config_json,
               updated_at = excluded.updated_at",
            params![organization_id, tunnel_id, tunnel.name, kind, secret_ref, config_json, at],
        )
        .map_err(backend)?;
        let stale_ref = old_ref.filter(|referenced| {
            !referenced.is_empty() && Some(referenced.as_str()) != secret_ref.as_deref()
        });
        tx.commit().map_err(backend)?;
        drop(conn);
        // 锁外清孤儿材料（vault 与本仓储共锁同一把连接）。
        if let Some(old_ref) = stale_ref {
            self.vault
                .sync_purge(organization_id, &ref_of(&old_ref))
                .map_err(|error| error)?;
        }
        Ok(())
    }

    fn sync_delete(&self, organization_id: &str, tunnel_id: &str) -> Result<(), PortError> {
        let mut conn = self.store.lock_for_adapter().map_err(store_backend)?;
        let tx = conn.transaction().map_err(backend)?;
        let secret_ref: Option<String> = tx
            .query_row(
                "SELECT secret_ref FROM tunnel_definition
                 WHERE organization_id = ?1 AND tunnel_id = ?2",
                params![organization_id, tunnel_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(backend)?;
        let affected = tx
            .execute(
                "DELETE FROM tunnel_definition
                 WHERE organization_id = ?1 AND tunnel_id = ?2",
                params![organization_id, tunnel_id],
            )
            .map_err(backend)?;
        if affected == 0 {
            return Err(PortError::NotFound("tunnel".into()));
        }
        let purge_ref = secret_ref.filter(|referenced| !referenced.is_empty());
        tx.commit().map_err(backend)?;
        drop(conn);
        // 锁外清秘密材料（vault 与本仓储共锁同一把连接）。
        if let Some(secret_ref) = purge_ref {
            self.vault
                .sync_purge(organization_id, &ref_of(&secret_ref))
                .map_err(|error| error)?;
        }
        Ok(())
    }

    fn sync_usage(&self, organization_id: &str, tunnel_id: &str) -> Result<TunnelUsage, PortError> {
        let conn = self.store.lock_for_adapter().map_err(store_backend)?;
        let mut stmt = conn
            .prepare(
                "SELECT connection_id, name FROM connection_catalog
                 WHERE organization_id = ?1 AND tunnel_id = ?2
                 ORDER BY sort_order, connection_id",
            )
            .map_err(backend)?;
        let rows = stmt
            .query_map(params![organization_id, tunnel_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(backend)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(backend)?;
        Ok(TunnelUsage {
            connection_ids: rows.iter().map(|(id, _)| id.clone()).collect(),
            connection_names: rows.into_iter().map(|(_, name)| name).collect(),
        })
    }
}

#[async_trait]
impl TunnelRepository for ServerTunnels {
    async fn list(&self, scope: &OrganizationScope) -> Result<Vec<SavedTunnelSummary>, PortError> {
        let this = self.clone();
        let organization_id = scope.organization_id.as_str().to_string();
        tokio::task::spawn_blocking(move || this.sync_list(&organization_id))
            .await
            .map_err(|source| PortError::BackendUnavailable(source.to_string()))?
    }

    async fn get(
        &self,
        scope: &OrganizationScope,
        tunnel_id: &str,
    ) -> Result<Option<SavedTunnel>, PortError> {
        let this = self.clone();
        let organization_id = scope.organization_id.as_str().to_string();
        let tunnel_id = tunnel_id.to_string();
        tokio::task::spawn_blocking(move || this.sync_get(&organization_id, &tunnel_id))
            .await
            .map_err(|source| PortError::BackendUnavailable(source.to_string()))?
    }

    async fn save(&self, scope: &OrganizationScope, tunnel: SavedTunnel) -> Result<(), PortError> {
        let this = self.clone();
        let organization_id = scope.organization_id.as_str().to_string();
        tokio::task::spawn_blocking(move || this.sync_save(&organization_id, tunnel))
            .await
            .map_err(|source| PortError::BackendUnavailable(source.to_string()))?
    }

    async fn delete(&self, scope: &OrganizationScope, tunnel_id: &str) -> Result<(), PortError> {
        let this = self.clone();
        let organization_id = scope.organization_id.as_str().to_string();
        let tunnel_id = tunnel_id.to_string();
        tokio::task::spawn_blocking(move || this.sync_delete(&organization_id, &tunnel_id))
            .await
            .map_err(|source| PortError::BackendUnavailable(source.to_string()))?
    }

    async fn usage(
        &self,
        scope: &OrganizationScope,
        tunnel_id: &str,
    ) -> Result<TunnelUsage, PortError> {
        let this = self.clone();
        let organization_id = scope.organization_id.as_str().to_string();
        let tunnel_id = tunnel_id.to_string();
        tokio::task::spawn_blocking(move || this.sync_usage(&organization_id, &tunnel_id))
            .await
            .map_err(|source| PortError::BackendUnavailable(source.to_string()))?
    }
}
