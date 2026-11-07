//! 查询资产双表的服务端 SQLite 适配器（W8 刀3a；契约 §6.12 的存储半边）。
//!
//! 词汇（[`FavoriteQuery`] / [`QueryHistoryEntry`]）唯一定义在共享层
//! `datazen::history`（桌面文件形与本表形共用一条 wire 词汇）；本件只
//! 负责 v10 双表的行存取与隔离边界：
//!
//! - **per (organization_id, principal_id) 隔离**（v9 named_views 同款
//!   口径）：[`ServerQueryFavoritesStore::scoped`] /
//!   [`ServerQueryFavoritesStore::for_identity`] 以构造参数携带隔离边界，
//!   HTTP handler 按认证链事实逐请求装配（INV-01）；越界读取天然落空
//!   （CM-05：读越界空），写只落本边界（WHERE 全带双键）。
//! - **id 服务端铸造**（uuid v4）——创建与历史落账都不收客户端 id。
//! - **异步边界**：与 `storage::settings` / named_views 同款——同步核心
//!   + `spawn_blocking` 包装。
//! - **历史保留策略**：追加后按本边界截断到
//!   [`MAX_QUERY_HISTORY`]（共享层常量；同一条业务规则的存储侧执行）。
//!   写入失败向上抛（`HistoryStoreError`），由调用方决定降级语义——
//!   执行面落账失败**不阻断执行应答**（best-effort + warn，桌面
//!   `record_sql_command_outcome` 同款），路由读面失败 → 503。
//!
//! 查询历史的读取过滤（`QueryHistoryFilter`）与分页形状
//! （`QueryHistoryPage`）直接复用共享层词汇——`search` 在 `limit` 之前
//! 施加（端口文档：调用方不必为找一行而放宽页），`since`/`until` 是
//! RFC3339 时刻，`order` 三态（recent/oldest/slowest）。

use std::sync::Arc;

use chrono::{DateTime, SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension};

use datazen::history::{
    FavoriteDraft, FavoriteQuery, HistoryOrder, HistoryScope, QueryHistoryEntry, QueryHistoryPage,
    MAX_QUERY_HISTORY,
};

use crate::storage::AdminStore;

/// 存储面错误（路由读面 → 503；落账面 → warn 降级）。
#[derive(Debug, thiserror::Error)]
pub enum HistoryStoreError {
    #[error("query assets store: {0}")]
    Backend(String),
}

fn backend(error: impl std::fmt::Display) -> HistoryStoreError {
    HistoryStoreError::Backend(error.to_string())
}

/// 记录时刻 → 定宽 RFC3339 UTC 毫秒（v5 起的适配器边界规范化口径）。
fn timestamp(at: DateTime<Utc>) -> String {
    at.to_rfc3339_opts(SecondsFormat::Millis, true)
}

// ── 收藏 ────────────────────────────────────────────────────────────────────

/// 收藏的 scoped 存取（v10 `query_favorites` 表；per (org, principal)）。
#[derive(Clone)]
pub struct ServerQueryFavoritesStore {
    store: Arc<AdminStore>,
    organization_id: String,
    principal_id: String,
}

impl ServerQueryFavoritesStore {
    /// 按隔离边界构造（HTTP handler 按认证链事实逐请求装配）。
    pub fn for_identity(store: Arc<AdminStore>, organization_id: &str, principal_id: &str) -> Self {
        Self {
            store,
            organization_id: organization_id.to_owned(),
            principal_id: principal_id.to_owned(),
        }
    }

    /// 列出本边界的收藏；`connection_id` 在场时按连接过滤（桌面
    /// `get_favorite_queries(connection_id)` 同语义）。
    pub async fn list(
        &self,
        connection_id: Option<&str>,
    ) -> Result<Vec<FavoriteQuery>, HistoryStoreError> {
        let store = self.store.clone();
        let org = self.organization_id.clone();
        let principal = self.principal_id.clone();
        let connection = connection_id.map(str::to_owned);
        tokio::task::spawn_blocking(move || {
            let conn = store.lock_for_adapter().map_err(backend)?;
            list_sync(&conn, &org, &principal, connection.as_deref())
        })
        .await
        .map_err(|error| backend(error.to_string()))?
    }

    /// 新建收藏：id 服务端铸造，落本边界。返回完整记录（含铸造 id 与
    /// 时刻）。
    pub async fn add(&self, draft: FavoriteDraft) -> Result<FavoriteQuery, HistoryStoreError> {
        let store = self.store.clone();
        let org = self.organization_id.clone();
        let principal = self.principal_id.clone();
        tokio::task::spawn_blocking(move || {
            let conn = store.lock_for_adapter().map_err(backend)?;
            add_sync(&conn, &org, &principal, draft)
        })
        .await
        .map_err(|error| backend(error.to_string()))?
    }

    /// 删除本边界内的一条收藏；返回移除行数（`Ok(0)` = id 不在边界内或
    /// 不存在——幂等 no-op，防枚举口径不区分二者）。
    pub async fn delete(&self, id: &str) -> Result<u64, HistoryStoreError> {
        let store = self.store.clone();
        let org = self.organization_id.clone();
        let principal = self.principal_id.clone();
        let id = id.to_owned();
        tokio::task::spawn_blocking(move || {
            let conn = store.lock_for_adapter().map_err(backend)?;
            let gone = conn
                .execute(
                    "DELETE FROM query_favorites WHERE id = ?1 AND organization_id = ?2 AND principal_id = ?3",
                    params![id, org, principal],
                )
                .map_err(backend)?;
            Ok(gone as u64)
        })
        .await
        .map_err(|error| backend(error.to_string()))?
    }
}

fn list_sync(
    conn: &Connection,
    org: &str,
    principal: &str,
    connection_id: Option<&str>,
) -> Result<Vec<FavoriteQuery>, HistoryStoreError> {
    // created_at 升序（桌面文件扫描按目录序的自然稳定口径的 DB 对偶；
    // 面板自行按需排序）。
    let mut statement = if connection_id.is_some() {
        conn.prepare(
            "SELECT id, connection_id, title, sql_text, database_name, keyword, folder, \
             created_at, updated_at FROM query_favorites \
             WHERE organization_id = ?1 AND principal_id = ?2 AND connection_id = ?3 \
             ORDER BY created_at, id",
        )
        .map_err(backend)?
    } else {
        conn.prepare(
            "SELECT id, connection_id, title, sql_text, database_name, keyword, folder, \
             created_at, updated_at FROM query_favorites \
             WHERE organization_id = ?1 AND principal_id = ?2 \
             ORDER BY created_at, id",
        )
        .map_err(backend)?
    };
    let map_row = |row: &rusqlite::Row| -> rusqlite::Result<FavoriteQuery> {
        let created_at: String = row.get(7)?;
        let updated_at: Option<String> = row.get(8)?;
        Ok(FavoriteQuery {
            id: row.get(0)?,
            connection_id: row.get(1)?,
            title: row.get(2)?,
            sql: row.get(3)?,
            created_at: parse_row_timestamp(&created_at)?,
            updated_at: updated_at
                .map(|raw| parse_row_timestamp(&raw))
                .transpose()?,
            keyword: row.get(5)?,
            database: row.get(4)?,
            folder: row.get(6)?,
        })
    };
    let rows = if let Some(connection) = connection_id {
        statement
            .query_map(params![org, principal, connection], map_row)
            .map_err(backend)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(backend)?
    } else {
        statement
            .query_map(params![org, principal], map_row)
            .map_err(backend)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(backend)?
    };
    Ok(rows)
}

fn add_sync(
    conn: &Connection,
    org: &str,
    principal: &str,
    draft: FavoriteDraft,
) -> Result<FavoriteQuery, HistoryStoreError> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = timestamp(Utc::now());
    conn.execute(
        "INSERT INTO query_favorites \
         (id, organization_id, principal_id, connection_id, title, sql_text, database_name, \
         keyword, folder, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, NULL)",
        params![
            id,
            org,
            principal,
            draft.connection_id,
            draft.title,
            draft.sql,
            draft.database,
            draft.keyword,
            draft.folder,
            now,
        ],
    )
    .map_err(backend)?;
    Ok(FavoriteQuery {
        id,
        connection_id: draft.connection_id,
        title: draft.title,
        sql: draft.sql,
        created_at: Utc::now(),
        updated_at: None,
        keyword: draft.keyword,
        database: draft.database,
        folder: draft.folder,
    })
}

// ── 查询历史 ────────────────────────────────────────────────────────────────

/// 查询历史的 scoped 存取（v10 `query_history` 表；per (org, principal)）。
#[derive(Clone)]
pub struct ServerQueryHistoryStore {
    store: Arc<AdminStore>,
    organization_id: String,
    principal_id: String,
}

/// 分页读取的过滤词面（与共享层 `QueryHistoryFilter<'a>` 同字段；本件收
/// owned 版本以便 spawn_blocking 转移）。
#[derive(Debug, Clone, Default)]
pub struct HistoryPageQuery {
    pub limit: usize,
    pub connection_id: Option<String>,
    pub database: Option<String>,
    pub schema: Option<String>,
    pub search: Option<String>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub order: Option<HistoryOrder>,
}

impl ServerQueryHistoryStore {
    /// 按隔离边界构造（执行面落账与路由读面同款装配）。
    pub fn for_identity(store: Arc<AdminStore>, organization_id: &str, principal_id: &str) -> Self {
        Self {
            store,
            organization_id: organization_id.to_owned(),
            principal_id: principal_id.to_owned(),
        }
    }

    /// 落账一条（id 服务端铸造；追加后按本边界截断到
    /// [`MAX_QUERY_HISTORY`]）。
    pub async fn add(&self, entry: QueryHistoryEntry) -> Result<(), HistoryStoreError> {
        let store = self.store.clone();
        let org = self.organization_id.clone();
        let principal = self.principal_id.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = store.lock_for_adapter().map_err(backend)?;
            add_history_sync(&mut conn, &org, &principal, entry)
        })
        .await
        .map_err(|error| backend(error.to_string()))?
    }

    /// 分页读取（含截断前 `total`）。
    pub async fn page(
        &self,
        query: HistoryPageQuery,
    ) -> Result<QueryHistoryPage, HistoryStoreError> {
        let store = self.store.clone();
        let org = self.organization_id.clone();
        let principal = self.principal_id.clone();
        tokio::task::spawn_blocking(move || {
            let conn = store.lock_for_adapter().map_err(backend)?;
            page_sync(&conn, &org, &principal, query)
        })
        .await
        .map_err(|error| backend(error.to_string()))?
    }

    /// 删除一条；返回移除行数（`Ok(0)` = 未知 id，调用方决定语义——
    /// 桌面 `delete_query_history` 同款「行数让调用方拒绝 no-op」）。
    pub async fn delete(&self, id: &str) -> Result<u64, HistoryStoreError> {
        let store = self.store.clone();
        let org = self.organization_id.clone();
        let principal = self.principal_id.clone();
        let id = id.to_owned();
        tokio::task::spawn_blocking(move || {
            let conn = store.lock_for_adapter().map_err(backend)?;
            let gone = conn
                .execute(
                    "DELETE FROM query_history WHERE id = ?1 AND organization_id = ?2 AND principal_id = ?3",
                    params![id, org, principal],
                )
                .map_err(backend)?;
            Ok(gone as u64)
        })
        .await
        .map_err(|error| backend(error.to_string()))?
    }

    /// 按作用域清除（`retain_days` 为 `None` 时全清）；返回移除行数。
    pub async fn purge(
        &self,
        scope: HistoryScope,
        retain_days: Option<u32>,
    ) -> Result<u64, HistoryStoreError> {
        let store = self.store.clone();
        let org = self.organization_id.clone();
        let principal = self.principal_id.clone();
        tokio::task::spawn_blocking(move || {
            let conn = store.lock_for_adapter().map_err(backend)?;
            purge_sync(&conn, &org, &principal, scope, retain_days)
        })
        .await
        .map_err(|error| backend(error.to_string()))?
    }
}

/// 落账同步核心（截断在同一连接内完成）。
fn add_history_sync(
    conn: &mut Connection,
    org: &str,
    principal: &str,
    entry: QueryHistoryEntry,
) -> Result<(), HistoryStoreError> {
    let tx = conn.transaction().map_err(backend)?;
    tx.execute(
        "INSERT INTO query_history \
         (id, organization_id, principal_id, connection_id, database_name, schema_name, \
         sql_text, executed_at, execution_time_ms, rows_affected, success, error_message) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            entry.id,
            org,
            principal,
            entry.connection_id,
            entry.database,
            entry.schema,
            entry.sql,
            timestamp(entry.executed_at),
            entry.execution_time_ms as i64,
            entry.rows_affected.map(|value| value as i64),
            entry.success as i64,
            entry.error_message,
        ],
    )
    .map_err(backend)?;
    // 保留截断：本边界内按 executed_at 降序保留前 MAX_QUERY_HISTORY 行。
    tx.execute(
        "DELETE FROM query_history WHERE id IN (\
         SELECT id FROM query_history \
         WHERE organization_id = ?1 AND principal_id = ?2 \
         ORDER BY executed_at DESC, id LIMIT -1 OFFSET ?3)",
        params![org, principal, MAX_QUERY_HISTORY],
    )
    .map_err(backend)?;
    tx.commit().map_err(backend)?;
    Ok(())
}

/// 分页同步核心：过滤在 LIMIT 之前，total 在过滤后、分页前计数。
fn page_sync(
    conn: &Connection,
    org: &str,
    principal: &str,
    query: HistoryPageQuery,
) -> Result<QueryHistoryPage, HistoryStoreError> {
    let mut where_clauses = vec![
        "organization_id = ?1".to_owned(),
        "principal_id = ?2".to_owned(),
    ];
    let mut bind_values: Vec<Box<dyn rusqlite::types::ToSql>> =
        vec![Box::new(org.to_owned()), Box::new(principal.to_owned())];
    if let Some(connection) = query.connection_id.as_deref() {
        where_clauses.push(format!("connection_id = ?{}", bind_values.len() + 1));
        bind_values.push(Box::new(connection.to_owned()));
    }
    if let Some(database) = query.database.as_deref() {
        where_clauses.push(format!("database_name = ?{}", bind_values.len() + 1));
        bind_values.push(Box::new(database.to_owned()));
    }
    if let Some(schema) = query.schema.as_deref() {
        where_clauses.push(format!("schema_name = ?{}", bind_values.len() + 1));
        bind_values.push(Box::new(schema.to_owned()));
    }
    if let Some(search) = query.search.as_deref() {
        where_clauses.push(format!("sql_text LIKE ?{}", bind_values.len() + 1));
        // 子串匹配（LIKE 通配转义交由驱动逐字面处理——历史搜索是运维
        // 面宽口径，桌面 history_db 的 LIKE 同款）。
        bind_values.push(Box::new(format!("%{search}%")));
    }
    if let Some(since) = query.since.as_deref() {
        where_clauses.push(format!("executed_at >= ?{}", bind_values.len() + 1));
        bind_values.push(Box::new(since.to_owned()));
    }
    if let Some(until) = query.until.as_deref() {
        where_clauses.push(format!("executed_at <= ?{}", bind_values.len() + 1));
        bind_values.push(Box::new(until.to_owned()));
    }
    let where_sql = where_clauses.join(" AND ");
    let order_sql = match query.order.unwrap_or_default() {
        HistoryOrder::Recent => "executed_at DESC, id DESC",
        HistoryOrder::Oldest => "executed_at, id",
        HistoryOrder::Slowest => "execution_time_ms DESC, id",
    };

    let total: u64 = conn
        .query_row(
            &format!("SELECT COUNT(*) FROM query_history WHERE {where_sql}"),
            rusqlite::params_from_iter(bind_values.iter().map(|value| value.as_ref())),
            |row| row.get::<_, i64>(0),
        )
        .map(|value| value as u64)
        .map_err(backend)?;

    let limit = query.limit.clamp(1, MAX_QUERY_HISTORY);
    let sql = format!(
        "SELECT id, connection_id, database_name, schema_name, sql_text, executed_at, \
         execution_time_ms, rows_affected, success, error_message \
         FROM query_history WHERE {where_sql} ORDER BY {order_sql} LIMIT {limit}"
    );
    let mut statement = conn.prepare(&sql).map_err(backend)?;
    let entries = statement
        .query_map(
            rusqlite::params_from_iter(bind_values.iter().map(|value| value.as_ref())),
            |row| {
                let executed_at: String = row.get(5)?;
                Ok(QueryHistoryEntry {
                    id: row.get(0)?,
                    connection_id: row.get(1)?,
                    database: row.get(2)?,
                    schema: row.get(3)?,
                    sql: row.get(4)?,
                    executed_at: parse_row_timestamp(&executed_at)?,
                    execution_time_ms: row.get::<_, i64>(6)? as u64,
                    rows_affected: row.get::<_, Option<i64>>(7)?.map(|v| v as u64),
                    success: row.get::<_, i64>(8)? != 0,
                    error_message: row.get(9)?,
                })
            },
        )
        .map_err(backend)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(backend)?;
    Ok(QueryHistoryPage { entries, total })
}

/// 作用域清除同步核心（`query` 清查询历史；`workflow` / `all` 的
/// workflow 历史词汇属 Workflow 功能族，本表未承载——按 0 行回答，
/// 语义如实）。
fn purge_sync(
    conn: &Connection,
    org: &str,
    principal: &str,
    scope: HistoryScope,
    retain_days: Option<u32>,
) -> Result<u64, HistoryStoreError> {
    // 本表只承载查询历史；workflow 历史词汇到达时由其自己的存储回答。
    if matches!(scope, HistoryScope::Workflow) {
        return Ok(0);
    }
    let gone = match retain_days {
        None => conn
            .execute(
                "DELETE FROM query_history WHERE organization_id = ?1 AND principal_id = ?2",
                params![org, principal],
            )
            .map_err(backend)?,
        Some(days) => {
            let cutoff = Utc::now() - chrono::Duration::days(i64::from(days));
            conn.execute(
                "DELETE FROM query_history WHERE organization_id = ?1 AND principal_id = ?2 \
                 AND executed_at < ?3",
                params![org, principal, timestamp(cutoff)],
            )
            .map_err(backend)?
        }
    };
    Ok(gone as u64)
}

/// 行内时刻解码（rusqlite 行映射闭包内的错误面）：坏时刻 = 行损坏，
/// 以 `FromSqlConversionFailure` 上抛——本域读取是整页查询，一行坏
/// 时刻让整页失败（503 如实暴露，不静默丟行）。
fn parse_row_timestamp(raw: &str) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(raw)
        .map(|parsed| parsed.with_timezone(&Utc))
        .map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("bad timestamp: {error}"),
                )),
            )
        })
}
