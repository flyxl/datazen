//! `NamedViewRepository` 端口的服务端 SQLite 适配器（契约 §6.9 命名视图的
//! 存储半边）。
//!
//! 端口（`datazen::named_views` 的 [`NamedViewRepository`]）**没有** scope
//! 参数——whole-collection 端口（桌面宿主整文件原子写，见端口模块头），
//! 组织与主体隔离由宿主按「每 (organization_id, principal_id) 一个实例」
//! 实现，[`ServerNamedViewStore`] 以构造参数携带隔离边界
//! （[`ServerNamedViewStore::scoped`]，HTTP handler 按会话组织的 `ctx`
//! 逐请求装配；INV-01：组织与主体只来自认证链事实，不来自请求体）。
//! §6.9 的归属裁决是 **per principal + scope**：同一组织的两个主体各自
//! 持有自己的视图集合，跨主体/跨组织读取天然落空（CM-05：读越界空/None）。
//!
//! 存储形状（v9 `named_views` 表）：
//!
//! - **整集合替换写**：端口语义是 `save_views(&[NamedView])` 替换整个集合，
//!   适配器在一个事务内「删除本实例边界内的旧行 + 插入全部新行」——与
//!   桌面 `named_views.json` 的整文件原子写同语义，记录上的 `created_at` /
//!   `updated_at`（application 层维护）是权威，行上的时刻列只是投影。
//! - **写越界拒绝**：id 服务端铸造（uuid v4），跨边界主键碰撞在 INSERT
//!   处显式失败（`NamedViewError::Storage`，事务回滚）——绝不静默改写
//!   他人行；DELETE 的「未知 id 无操作」由 application 层幂等语义保证
//!   （§6.9：未知 id 删除是幂等 no-op，返回剩余集合），适配器不另造
//!   NotFound 分支。
//! - **scope 绑定持久 connectionId**：`NamedViewScope.connection_id` 是
//!   持久连接身份，落 NOT NULL 投影列；`dbSessionId` 是运行时状态，本表
//!   没有也不允许有会话列（§6.9 明文）。`database` / `schema` 可空。
//! - **权威是 `view_json`**：整条 `NamedView` 的 JSON 投影（v3/v4/v7 同款
//!   JSONB→TEXT 口径）；投影列只服务查询与可观测。损坏行读取期**跳过**
//!   而非致命（端口文档：一条损坏的记录不得藏起它周围的视图）。
//!
//! 异步边界：与 `storage::settings` 同款——同步核心 + `spawn_blocking` 包装。

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, SecondsFormat, Utc};
use rusqlite::params;

use datazen::named_views::{NamedView, NamedViewError, NamedViewIdMinter, NamedViewRepository};

use crate::storage::AdminStore;

fn backend(error: impl std::fmt::Display) -> NamedViewError {
    NamedViewError::Storage(error.to_string())
}

/// 记录时刻 → 定宽 RFC3339 UTC 毫秒（`shared::clock` 单一形状，v5 起的
/// 适配器边界规范化口径）。
fn timestamp(at: &DateTime<Utc>) -> String {
    at.to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// 每 (organization_id, principal_id) 一个实例的命名视图存储适配器。
#[derive(Clone)]
pub struct ServerNamedViewStore {
    store: Arc<AdminStore>,
    organization_id: String,
    principal_id: String,
}

impl ServerNamedViewStore {
    /// 在已迁移的管理库上构造（调用方负责先跑 `AdminStore::migrate`——v9
    /// 起有 `named_views`）。
    pub fn new(store: Arc<AdminStore>, organization_id: String, principal_id: String) -> Self {
        Self {
            store,
            organization_id,
            principal_id,
        }
    }

    /// 按请求上下文的组织与主体装配（HTTP handler 的唯一构造入口；两者
    /// 都来自认证链盖章的 `ctx`，INV-01）。
    pub fn scoped(organization_id: &str, principal_id: &str, store: Arc<AdminStore>) -> Self {
        Self::new(store, organization_id.to_string(), principal_id.to_string())
    }

    fn sync_load_views(&self) -> Result<Vec<NamedView>, NamedViewError> {
        let conn = self
            .store
            .lock_for_adapter()
            .map_err(|error| backend(error))?;
        let mut stmt = conn
            .prepare(
                "SELECT id, view_json FROM named_views
                 WHERE organization_id = ?1 AND principal_id = ?2",
            )
            .map_err(backend)?;
        let rows = stmt
            .query_map(params![self.organization_id, self.principal_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(backend)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(backend)?;
        let mut views = Vec::new();
        for (id, json) in rows {
            match serde_json::from_str::<NamedView>(&json) {
                Ok(view) => views.push(view),
                Err(error) => {
                    // 损坏行跳过而非致命（端口契约：一条损坏的记录不得
                    // 藏起它周围的视图）。id 是服务端铸造的非敏感事实。
                    tracing::warn!(view_id = %id, %error, "stored named view is corrupt; skipping");
                }
            }
        }
        Ok(views)
    }

    fn sync_save_views(&self, views: &[NamedView]) -> Result<(), NamedViewError> {
        let mut conn = self
            .store
            .lock_for_adapter()
            .map_err(|error| backend(error))?;
        let tx = conn.transaction().map_err(backend)?;
        // 整集合替换的第一半：只删本实例边界内的行（org + principal 双列
        // 限定），越界行不可达。
        tx.execute(
            "DELETE FROM named_views WHERE organization_id = ?1 AND principal_id = ?2",
            params![self.organization_id, self.principal_id],
        )
        .map_err(backend)?;
        for view in views {
            let json = serde_json::to_string(view).map_err(|error| {
                backend(format!("stored named view is not serializable: {error}"))
            })?;
            let scope = &view.scope;
            // 第二半：逐行插入。id 撞上**其他**边界（其他组织/主体）的行
            // 时主键冲突 → 显式 Storage 拒绝，事务整体回滚（写越界拒绝，
            // 见模块头）。
            tx.execute(
                "INSERT INTO named_views
                   (id, organization_id, principal_id, connection_id,
                    database_name, schema_name, table_name, view_json,
                    created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    view.id,
                    self.organization_id,
                    self.principal_id,
                    scope.connection_id,
                    scope.database,
                    scope.schema,
                    scope.table,
                    json,
                    timestamp(&view.created_at),
                    view.updated_at.as_ref().map(timestamp),
                ],
            )
            .map_err(backend)?;
        }
        tx.commit().map_err(backend)?;
        Ok(())
    }
}

#[async_trait]
impl NamedViewRepository for ServerNamedViewStore {
    async fn load_views(&self) -> Result<Vec<NamedView>, NamedViewError> {
        let this = self.clone();
        tokio::task::spawn_blocking(move || this.sync_load_views())
            .await
            .map_err(|source| backend(format!("named views read task failed: {source}")))?
    }

    async fn save_views(&self, views: &[NamedView]) -> Result<(), NamedViewError> {
        let views = views.to_vec();
        let this = self.clone();
        tokio::task::spawn_blocking(move || this.sync_save_views(&views))
            .await
            .map_err(|source| backend(format!("named views write task failed: {source}")))?
    }
}

/// `NamedViewIdMinter` 的服务端形态：CSPRNG uuid v4（桌面铸 ULID 的
/// server 对位；全局唯一，`named_views.id` 单列主键由此安全）。
pub struct UuidNamedViewIdMinter;

impl NamedViewIdMinter for UuidNamedViewIdMinter {
    fn mint_id(&self) -> String {
        uuid::Uuid::new_v4().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen::named_views::{NamedViewRepository as _, NamedViewScope};

    fn store() -> Arc<AdminStore> {
        let store = AdminStore::open_in_memory().expect("store");
        store.migrate().expect("migrate applies v1..=v9");
        Arc::new(store)
    }

    fn view(id: &str, name: &str, connection: &str, table: &str, default: bool) -> NamedView {
        NamedView {
            id: id.to_owned(),
            name: name.to_owned(),
            scope: NamedViewScope {
                connection_id: connection.to_owned(),
                database: Some("app".into()),
                schema: None,
                table: table.to_owned(),
            },
            filters: Vec::new(),
            filter_logic: "and".into(),
            sorts: Vec::new(),
            visible_columns: None,
            column_order: None,
            is_default: default,
            created_at: Utc::now(),
            updated_at: None,
        }
    }

    fn scoped(org: &str, principal: &str, store: &Arc<AdminStore>) -> ServerNamedViewStore {
        ServerNamedViewStore::scoped(org, principal, store.clone())
    }

    #[tokio::test]
    async fn a_saved_collection_round_trips_through_the_rows() {
        let store = store();
        let owner = scoped("org-1", "user-1", &store);
        let mut views = vec![view("", "Alpha", "conn-1", "orders", true)];
        views[0].id = "view-alpha".into();
        owner.save_views(&views).await.expect("save");

        let loaded = owner.load_views().await.expect("load");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, "view-alpha");
        assert_eq!(loaded[0].scope.connection_id, "conn-1");
        assert_eq!(loaded[0].scope.database.as_deref(), Some("app"));
        assert_eq!(loaded[0].scope.schema, None);
        assert_eq!(loaded[0].scope.table, "orders");
    }

    #[tokio::test]
    async fn another_principals_collection_is_invisible_and_a_colliding_write_is_refused() {
        let store = store();
        let owner = scoped("org-1", "user-1", &store);
        let other = scoped("org-1", "user-2", &store);
        let foreign_org = scoped("org-2", "user-1", &store);

        owner
            .save_views(&[view("view-x", "Alpha", "conn-1", "orders", false)])
            .await
            .expect("save");

        // CM-05 读越界：别的主体、别的组织读到的都是空集合。
        assert!(other.load_views().await.expect("load").is_empty());
        assert!(foreign_org.load_views().await.expect("load").is_empty());

        // 写越界：撞他人 id 的写入显式失败（不静默改写）。
        let collision = other
            .save_views(&[view("view-x", "Alpha", "conn-1", "orders", false)])
            .await;
        assert!(collision.is_err(), "a cross-boundary id write must fail");

        // 失败写不产生副作用：原记录原样保留。
        let loaded = owner.load_views().await.expect("reload");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "Alpha");
    }

    #[tokio::test]
    async fn a_corrupt_row_is_skipped_instead_of_hiding_the_rest() {
        let store = store();
        let owner = scoped("org-1", "user-1", &store);
        owner
            .save_views(&[
                view("view-a", "Alpha", "conn-1", "orders", false),
                view("view-b", "Beta", "conn-1", "orders", false),
            ])
            .await
            .expect("save");

        {
            let conn = store.lock_for_adapter().expect("lock");
            conn.execute(
                "UPDATE named_views SET view_json = '{not json' WHERE id = 'view-a'",
                [],
            )
            .expect("corrupt one row");
        }

        let loaded = owner.load_views().await.expect("load");
        assert_eq!(loaded.len(), 1, "the damaged entry is skipped");
        assert_eq!(loaded[0].id, "view-b");
    }
}
