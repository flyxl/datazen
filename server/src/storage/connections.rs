//! `ConnectionRepository`（连接目录表单词汇）的服务端适配器（WR-D2 收尾）。
//!
//! 端口（`datazen::connection::domain::catalog`）的形状是 `datazen-driver-api`
//! 的 [`ConnectionConfig`]——每个形态都已链接的那一份、连接表单的 IPC 载荷，
//! 服务端**不二次投影**（表单逐字回显）。
//!
//! 存储形状（v7 `connection_catalog` / `connection_catalog_group` + 复用
//! `stored_secret`）与裁决：
//!
//! - **凭据永不明文落库**：save 时 `extract_sensitive_fields` 摘走敏感子树
//!   （`password` / `passphrase` / `authToken`，含 ssh `jump` 嵌套），剩余
//!   净化投影落 `config_json`；摘走的明文经 [`ServerSecretVault`] 密封进
//!   `stored_secret`（`secret_ref = server-vault:connection:{org}:{id}`）。
//!   get 时反向拼回（`splice_sensitive_fields`），表单回显逐字一致。
//! - 投影列：`sort_order`（reorder 的用户排序）、`tunnel_id`、`group_name`
//!   （让 usage/分组反查走索引，不扫 JSON）；`last_connected_at` 由
//!   `touch_last_connected_at` 窄更新（避免整行写回的丢失更新）。
//! - 组织隔离沿 CM-05：读越界 = 空/None；写越界 = `NotFound`（不泄漏存在性）。
//!   端口的 scope 就是组织，save 在本组织内 upsert（无独立 NotFound 面）。
//! - delete 连带 `vault.sync_purge`（秘密材料随行消亡）；旧 secret_ref 换新
//!   时同样清掉孤儿材料。
//! - 分组：`groups` = 持久化自定义组 ∪ 连接行上实际出现的组（并集语义）；
//!   `save_groups` 整表替换自定义组（组没有身份，逐条 CRUD 会发明身份）。

use std::sync::Arc;

use async_trait::async_trait;
use datazen_driver_api::ConnectionConfig;
use rusqlite::{params, OptionalExtension};

use datazen::connection::domain::catalog::{
    ConnectionRepository, ConnectionRepositoryError, ConnectionScope,
};

use crate::execution::secret_provider::{
    extract_sensitive_fields, splice_sensitive_fields, ServerSecretVault,
};
use crate::storage::AdminStore;

/// 行投影（列序固定，与查询逐一对应）。
const CATALOG_COLUMNS: &str = "connection_id, name, group_name, tunnel_id, secret_ref, \
     config_json, sort_order, last_connected_at";

fn now_string() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn backend(error: rusqlite::Error) -> ConnectionRepositoryError {
    ConnectionRepositoryError::BackendUnavailable(error.to_string())
}

/// 连接目录的服务端适配器。每组织一个实例（scope 参数即组织边界）。
#[derive(Clone)]
pub struct ServerConnections {
    store: Arc<AdminStore>,
    vault: Arc<ServerSecretVault>,
}

impl std::fmt::Debug for ServerConnections {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerConnections")
            .field("store", &"AdminStore")
            .field("vault", &"ServerSecretVault")
            .finish()
    }
}

impl ServerConnections {
    pub fn new(store: Arc<AdminStore>, vault: Arc<ServerSecretVault>) -> Self {
        Self { store, vault }
    }

    // ── 同步核心（trait 实现只做 spawn_blocking 包装；profiles.rs 同款） ──

    /// 行 → `ConnectionConfig`：净化投影拼回敏感子树（`last_connected_at`
    /// 投影列回填进值树；存储被篡改时 `open` 先行失败）。
    fn config_from_row(
        &self,
        organization_id: &str,
        connection_id: &str,
        group_name: Option<String>,
        tunnel_id: Option<String>,
        secret_ref: Option<String>,
        config_json: String,
        last_connected_at: Option<String>,
    ) -> Result<ConnectionConfig, ConnectionRepositoryError> {
        let mut value: serde_json::Value = serde_json::from_str(&config_json).map_err(|error| {
            ConnectionRepositoryError::BackendUnavailable(format!(
                "catalog row is not parseable: {error}"
            ))
        })?;
        if let Some(secret_ref) = secret_ref {
            let revision = self
                .vault
                .sync_current_revision(organization_id, &ref_of(&secret_ref))
                .map_err(|error| ConnectionRepositoryError::BackendUnavailable(error.to_string()))?
                .ok_or_else(|| {
                    ConnectionRepositoryError::BackendUnavailable(format!(
                        "secret material missing for {connection_id}"
                    ))
                })?;
            let material = self
                .vault
                .sync_open_revision(organization_id, &ref_of(&secret_ref), revision)
                .map_err(|error| ConnectionRepositoryError::BackendUnavailable(error.to_string()))?
                .ok_or_else(|| {
                    ConnectionRepositoryError::BackendUnavailable(format!(
                        "secret revision missing for {connection_id}"
                    ))
                })?;
            let secrets: serde_json::Value =
                serde_json::from_slice(&material).map_err(|error| {
                    ConnectionRepositoryError::BackendUnavailable(format!(
                        "sealed material is not parseable: {error}"
                    ))
                })?;
            splice_sensitive_fields(&mut value, &secrets);
        }
        if let Some(last_connected_at) = last_connected_at {
            value["lastConnectedAt"] = serde_json::Value::String(last_connected_at);
        }
        // 投影列是权威（迁移文档登记的索引反查面）；值树缺省时回填。
        if let Some(group) = group_name {
            value["group"] = serde_json::Value::String(group);
        }
        if let Some(tunnel_id) = tunnel_id {
            value["tunnelId"] = serde_json::Value::String(tunnel_id);
        }
        serde_json::from_value(value).map_err(|error| {
            ConnectionRepositoryError::BackendUnavailable(format!(
                "catalog row does not fit the connection form: {error}"
            ))
        })
    }

    fn sync_get(
        &self,
        organization_id: &str,
        connection_id: &str,
    ) -> Result<Option<ConnectionConfig>, ConnectionRepositoryError> {
        // 读原始行在先（锁内），解封拼回在后（锁外）——vault 的 sync_* 会
        // 锁同一把 AdminStore 连接，锁内调用必然自锁死。
        let row = {
            let conn = self.store.lock_for_adapter().map_err(|error| {
                ConnectionRepositoryError::BackendUnavailable(error.to_string())
            })?;
            conn.query_row(
                &format!(
                    "SELECT {CATALOG_COLUMNS} FROM connection_catalog
                     WHERE organization_id = ?1 AND connection_id = ?2"
                ),
                params![organization_id, connection_id],
                |row| {
                    Ok((
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, i64>(6)?,
                        row.get::<_, Option<String>>(7)?,
                    ))
                },
            )
            .optional()
            .map_err(backend)?
        };
        let Some((name, group_name, tunnel_id, secret_ref, config_json, _sort, last_connected_at)) =
            row
        else {
            return Ok(None);
        };
        let mut config = self.config_from_row(
            organization_id,
            connection_id,
            group_name,
            tunnel_id,
            secret_ref,
            config_json,
            last_connected_at,
        )?;
        config.name = name;
        Ok(Some(config))
    }

    fn sync_list(
        &self,
        organization_id: &str,
    ) -> Result<Vec<ConnectionConfig>, ConnectionRepositoryError> {
        let ids: Vec<String> = {
            let conn = self.store.lock_for_adapter().map_err(|error| {
                ConnectionRepositoryError::BackendUnavailable(error.to_string())
            })?;
            let mut stmt = conn
                .prepare(
                    "SELECT connection_id FROM connection_catalog
                     WHERE organization_id = ?1
                     ORDER BY sort_order, connection_id",
                )
                .map_err(backend)?;
            // collect 与 `?` 分步绑定：块尾临时会活到块末（晚于 stmt/conn
            // 析构），先落成 owned 值再传播错误。
            let rows = stmt
                .query_map([organization_id], |row| row.get(0))
                .map_err(backend)?
                .collect::<Result<Vec<_>, _>>();
            rows.map_err(backend)?
        };
        // 逐行走 get 的拼回路径（同一份解封逻辑，不做第二份投影）。
        ids.into_iter()
            .map(|id| {
                self.sync_get(organization_id, &id)?.ok_or_else(|| {
                    ConnectionRepositoryError::BackendUnavailable(format!(
                        "catalog row {id} vanished mid-scan"
                    ))
                })
            })
            .collect()
    }

    fn sync_save(
        &self,
        organization_id: &str,
        config: ConnectionConfig,
    ) -> Result<(), ConnectionRepositoryError> {
        let connection_id = config.id.clone();
        // 表单词汇 → 值树；摘走敏感子树后剩余投影才是可落库形状。
        let mut value = serde_json::to_value(&config).map_err(|error| {
            ConnectionRepositoryError::BackendUnavailable(format!(
                "connection config is not serializable: {error}"
            ))
        })?;
        let secrets = extract_sensitive_fields(&mut value);
        let config_json = serde_json::to_string(&value).map_err(|error| {
            ConnectionRepositoryError::BackendUnavailable(format!(
                "sanitized projection is not serializable: {error}"
            ))
        })?;
        let group_name = config.group.clone().filter(|group| !group.is_empty());
        let tunnel_id = config.tunnel_id.clone().filter(|tunnel| !tunnel.is_empty());

        let has_material = secrets
            .as_object()
            .map(|map| !map.is_empty())
            .unwrap_or(false);
        let secret_ref = if has_material {
            let secret_ref =
                ServerSecretVault::mint_ref("connection", organization_id, &connection_id);
            let material = serde_json::to_vec(&secrets).map_err(|error| {
                ConnectionRepositoryError::BackendUnavailable(format!(
                    "sealed material is not serializable: {error}"
                ))
            })?;
            self.vault
                .sync_store_material(organization_id, &secret_ref, &material)
                .map_err(|error| {
                    ConnectionRepositoryError::BackendUnavailable(error.to_string())
                })?;
            Some(secret_ref.as_str().to_string())
        } else {
            None
        };

        let mut conn = self
            .store
            .lock_for_adapter()
            .map_err(|error| ConnectionRepositoryError::BackendUnavailable(error.to_string()))?;
        let tx = conn.transaction().map_err(backend)?;
        // 旧行的 secret_ref：换新/清空时清掉孤儿材料。
        let old_ref: Option<String> = tx
            .query_row(
                "SELECT secret_ref FROM connection_catalog
                 WHERE organization_id = ?1 AND connection_id = ?2",
                params![organization_id, connection_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(backend)?;
        let at = now_string();
        tx.execute(
            "INSERT INTO connection_catalog
               (organization_id, connection_id, name, group_name, tunnel_id, secret_ref,
                config_json, sort_order, last_connected_at, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7,
                     COALESCE((SELECT sort_order + 1 FROM connection_catalog
                               WHERE organization_id = ?1
                               ORDER BY sort_order DESC LIMIT 1), 0),
                     NULL, ?8, ?8)
             ON CONFLICT(organization_id, connection_id) DO UPDATE SET
               name = excluded.name,
               group_name = excluded.group_name,
               tunnel_id = excluded.tunnel_id,
               secret_ref = excluded.secret_ref,
               config_json = excluded.config_json,
               updated_at = excluded.updated_at",
            params![
                organization_id,
                connection_id,
                config.name,
                group_name,
                tunnel_id,
                secret_ref,
                config_json,
                at,
            ],
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
                .map_err(|error| {
                    ConnectionRepositoryError::BackendUnavailable(error.to_string())
                })?;
        }
        Ok(())
    }

    fn sync_delete(
        &self,
        organization_id: &str,
        connection_id: &str,
    ) -> Result<(), ConnectionRepositoryError> {
        let mut conn = self
            .store
            .lock_for_adapter()
            .map_err(|error| ConnectionRepositoryError::BackendUnavailable(error.to_string()))?;
        let tx = conn.transaction().map_err(backend)?;
        let secret_ref: Option<String> = tx
            .query_row(
                "SELECT secret_ref FROM connection_catalog
                 WHERE organization_id = ?1 AND connection_id = ?2",
                params![organization_id, connection_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(backend)?;
        let affected = tx
            .execute(
                "DELETE FROM connection_catalog
                 WHERE organization_id = ?1 AND connection_id = ?2",
                params![organization_id, connection_id],
            )
            .map_err(backend)?;
        if affected == 0 {
            return Err(ConnectionRepositoryError::NotFound(
                connection_id.to_string(),
            ));
        }
        let purge_ref = secret_ref.filter(|referenced| !referenced.is_empty());
        tx.commit().map_err(backend)?;
        drop(conn);
        // 锁外清秘密材料（vault 与本仓储共锁同一把连接）。
        if let Some(secret_ref) = purge_ref {
            self.vault
                .sync_purge(organization_id, &ref_of(&secret_ref))
                .map_err(|error| {
                    ConnectionRepositoryError::BackendUnavailable(error.to_string())
                })?;
        }
        Ok(())
    }

    fn sync_reorder(
        &self,
        organization_id: &str,
        ordered_ids: Vec<String>,
    ) -> Result<(), ConnectionRepositoryError> {
        let existing: Vec<String> = {
            let conn = self.store.lock_for_adapter().map_err(|error| {
                ConnectionRepositoryError::BackendUnavailable(error.to_string())
            })?;
            let mut stmt = conn
                .prepare(
                    "SELECT connection_id FROM connection_catalog
                     WHERE organization_id = ?1 ORDER BY sort_order, connection_id",
                )
                .map_err(backend)?;
            let rows = stmt
                .query_map([organization_id], |row| row.get(0))
                .map_err(backend)?
                .collect::<Result<Vec<_>, _>>();
            rows.map_err(backend)?
        };
        // 提及者按给定顺序在前（未知 id 忽略）；未提及者保持原相对顺序垫后。
        let mut next_order: Vec<String> = Vec::with_capacity(existing.len());
        let mut seen = std::collections::HashSet::new();
        for id in &ordered_ids {
            if existing.contains(id) && seen.insert(id.clone()) {
                next_order.push(id.clone());
            }
        }
        for id in &existing {
            if !seen.contains(id) {
                next_order.push(id.clone());
            }
        }
        let conn = self
            .store
            .lock_for_adapter()
            .map_err(|error| ConnectionRepositoryError::BackendUnavailable(error.to_string()))?;
        for (position, id) in next_order.iter().enumerate() {
            conn.execute(
                "UPDATE connection_catalog SET sort_order = ?3
                 WHERE organization_id = ?1 AND connection_id = ?2",
                params![organization_id, id, position as i64],
            )
            .map_err(backend)?;
        }
        Ok(())
    }

    fn sync_touch_last_connected_at(
        &self,
        organization_id: &str,
        connection_id: &str,
    ) -> Result<(), ConnectionRepositoryError> {
        let conn = self
            .store
            .lock_for_adapter()
            .map_err(|error| ConnectionRepositoryError::BackendUnavailable(error.to_string()))?;
        let affected = conn
            .execute(
                "UPDATE connection_catalog SET last_connected_at = ?3
                 WHERE organization_id = ?1 AND connection_id = ?2",
                params![organization_id, connection_id, now_string()],
            )
            .map_err(backend)?;
        if affected == 0 {
            return Err(ConnectionRepositoryError::NotFound(
                connection_id.to_string(),
            ));
        }
        Ok(())
    }

    fn sync_groups(&self, organization_id: &str) -> Result<Vec<String>, ConnectionRepositoryError> {
        let conn = self
            .store
            .lock_for_adapter()
            .map_err(|error| ConnectionRepositoryError::BackendUnavailable(error.to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT group_name FROM connection_catalog_group WHERE organization_id = ?1
                 UNION
                 SELECT DISTINCT group_name FROM connection_catalog
                 WHERE organization_id = ?1 AND group_name IS NOT NULL
                 ORDER BY 1",
            )
            .map_err(backend)?;
        let rows = stmt
            .query_map([organization_id], |row| row.get::<_, String>(0))
            .map_err(backend)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(backend)?;
        Ok(rows)
    }

    fn sync_save_groups(
        &self,
        organization_id: &str,
        groups: Vec<String>,
    ) -> Result<(), ConnectionRepositoryError> {
        let mut conn = self
            .store
            .lock_for_adapter()
            .map_err(|error| ConnectionRepositoryError::BackendUnavailable(error.to_string()))?;
        let tx = conn.transaction().map_err(backend)?;
        tx.execute(
            "DELETE FROM connection_catalog_group WHERE organization_id = ?1",
            [organization_id],
        )
        .map_err(backend)?;
        let at = now_string();
        for group in groups {
            if group.trim().is_empty() {
                continue; // CHECK (group_name <> '') 的前置过滤
            }
            tx.execute(
                "INSERT OR IGNORE INTO connection_catalog_group
                   (organization_id, group_name, created_at)
                 VALUES (?1, ?2, ?3)",
                params![organization_id, group, at],
            )
            .map_err(backend)?;
        }
        tx.commit().map_err(backend)?;
        Ok(())
    }
}

/// 存储列里的引用串 → `SecretRef`（vault 铸造的形状，逐字还原）。
fn ref_of(secret_ref: &str) -> datazen::shared::id::SecretRef {
    datazen::shared::id::SecretRef::new(secret_ref.to_string())
}

#[async_trait]
impl ConnectionRepository for ServerConnections {
    async fn list(
        &self,
        scope: &ConnectionScope,
    ) -> Result<Vec<ConnectionConfig>, ConnectionRepositoryError> {
        let this = self.clone();
        let organization_id = scope.organization_id().to_string();
        tokio::task::spawn_blocking(move || this.sync_list(&organization_id))
            .await
            .map_err(|source| ConnectionRepositoryError::BackendUnavailable(source.to_string()))?
    }

    async fn get(
        &self,
        scope: &ConnectionScope,
        connection_id: &str,
    ) -> Result<Option<ConnectionConfig>, ConnectionRepositoryError> {
        let this = self.clone();
        let organization_id = scope.organization_id().to_string();
        let connection_id = connection_id.to_string();
        tokio::task::spawn_blocking(move || this.sync_get(&organization_id, &connection_id))
            .await
            .map_err(|source| ConnectionRepositoryError::BackendUnavailable(source.to_string()))?
    }

    async fn save(
        &self,
        scope: &ConnectionScope,
        config: ConnectionConfig,
    ) -> Result<(), ConnectionRepositoryError> {
        let this = self.clone();
        let organization_id = scope.organization_id().to_string();
        tokio::task::spawn_blocking(move || this.sync_save(&organization_id, config))
            .await
            .map_err(|source| ConnectionRepositoryError::BackendUnavailable(source.to_string()))?
    }

    async fn delete(
        &self,
        scope: &ConnectionScope,
        connection_id: &str,
    ) -> Result<(), ConnectionRepositoryError> {
        let this = self.clone();
        let organization_id = scope.organization_id().to_string();
        let connection_id = connection_id.to_string();
        tokio::task::spawn_blocking(move || this.sync_delete(&organization_id, &connection_id))
            .await
            .map_err(|source| ConnectionRepositoryError::BackendUnavailable(source.to_string()))?
    }

    async fn reorder(
        &self,
        scope: &ConnectionScope,
        ordered_ids: Vec<String>,
    ) -> Result<(), ConnectionRepositoryError> {
        let this = self.clone();
        let organization_id = scope.organization_id().to_string();
        tokio::task::spawn_blocking(move || this.sync_reorder(&organization_id, ordered_ids))
            .await
            .map_err(|source| ConnectionRepositoryError::BackendUnavailable(source.to_string()))?
    }

    async fn touch_last_connected_at(
        &self,
        scope: &ConnectionScope,
        connection_id: &str,
    ) -> Result<(), ConnectionRepositoryError> {
        let this = self.clone();
        let organization_id = scope.organization_id().to_string();
        let connection_id = connection_id.to_string();
        tokio::task::spawn_blocking(move || {
            this.sync_touch_last_connected_at(&organization_id, &connection_id)
        })
        .await
        .map_err(|source| ConnectionRepositoryError::BackendUnavailable(source.to_string()))?
    }

    async fn groups(
        &self,
        scope: &ConnectionScope,
    ) -> Result<Vec<String>, ConnectionRepositoryError> {
        let this = self.clone();
        let organization_id = scope.organization_id().to_string();
        tokio::task::spawn_blocking(move || this.sync_groups(&organization_id))
            .await
            .map_err(|source| ConnectionRepositoryError::BackendUnavailable(source.to_string()))?
    }

    async fn save_groups(
        &self,
        scope: &ConnectionScope,
        groups: Vec<String>,
    ) -> Result<(), ConnectionRepositoryError> {
        let this = self.clone();
        let organization_id = scope.organization_id().to_string();
        tokio::task::spawn_blocking(move || this.sync_save_groups(&organization_id, groups))
            .await
            .map_err(|source| ConnectionRepositoryError::BackendUnavailable(source.to_string()))?
    }
}
