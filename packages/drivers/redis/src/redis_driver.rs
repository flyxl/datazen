//! Redis driver — exposes Redis as a key-value browser through the DatabaseDriver trait.
//!
//! Unlike SQL databases, Redis doesn't have tables/schemas. We map the concepts:
//! - "databases" → non-empty logical databases (`db0`, `db1`, …)
//! - "tables" → keys in the selected database (as `TableInfo` for tree browsing)
//! - "query" → raw Redis command execution (e.g. `GET key`, `"HGETALL" key` with quotes)
//! - `scan_keys_with_info` / `get_key_detail` — KV browser commands

use datazen_driver_api::*;
use redis::AsyncCommands;
use redis::FromRedisValue;
use std::collections::HashMap;
use std::time::Instant;
use tokio::sync::RwLock;

use crate::connect::{
    build_connection_plan, looks_like_connection_loss, open_live_conn, open_pinned_node_conn,
    ConnectionPlan, RedisLiveConn,
};
use crate::with_redis_conn;

struct RedisConn {
    plan: ConnectionPlan,
    live: RedisLiveConn,
}

macro_rules! with_live_op {
    ($self:expr, $connection_id:expr, $db_index:expr, |$conn:ident| $body:expr) => {{
        let handle = ConnectionHandle {
            id: $connection_id.to_string(),
            pool_id: $connection_id.to_string(),
        };
        let mut conns = $self.connections.write().await;
        let rc = RedisDriver::get_conn(&mut conns, &handle)?;
        RedisDriver::select_db(&mut rc.live, $db_index)
            .await
            .map_err(DriverError::QueryFailed)?;
        let mut result = with_redis_conn!(&mut rc.live, |$conn| $body);
        if result.is_err()
            && rc.live.is_sentinel()
            && looks_like_connection_loss(result.as_ref().unwrap_err())
        {
            rc.live.rediscover_sentinel_master().await?;
            RedisDriver::select_db(&mut rc.live, $db_index)
                .await
                .map_err(DriverError::QueryFailed)?;
            result = with_redis_conn!(&mut rc.live, |$conn| $body);
        }
        result.map_err(DriverError::QueryFailed)
    }};
}

macro_rules! with_live_any_op {
    ($self:expr, $connection_id:expr, |$conn:ident| $body:expr) => {{
        let handle = ConnectionHandle {
            id: $connection_id.to_string(),
            pool_id: $connection_id.to_string(),
        };
        let mut conns = $self.connections.write().await;
        let rc = RedisDriver::get_conn(&mut conns, &handle)?;
        let mut result = with_redis_conn!(&mut rc.live, |$conn| $body);
        if result.is_err()
            && rc.live.is_sentinel()
            && looks_like_connection_loss(result.as_ref().unwrap_err())
        {
            rc.live.rediscover_sentinel_master().await?;
            result = with_redis_conn!(&mut rc.live, |$conn| $body);
        }
        result.map_err(DriverError::QueryFailed)
    }};
}

macro_rules! plugin_on_db {
    ($name:ident, ($($arg:ident: $arg_ty:ty),*) -> $ret:ty, |$conn:ident| $body:expr) => {
        pub async fn $name(
            &self,
            connection_id: &str,
            db_index: u32,
            $($arg: $arg_ty,)*
        ) -> Result<$ret, DriverError> {
            with_live_op!(self, connection_id, db_index, |$conn| ($body).await)
        }
    };
}

pub struct RedisDriver {
    connections: RwLock<HashMap<String, RedisConn>>,
}

impl RedisDriver {
    pub fn new() -> Self {
        Self {
            connections: RwLock::new(HashMap::new()),
        }
    }

    fn get_conn<'a>(
        conns: &'a mut HashMap<String, RedisConn>,
        handle: &ConnectionHandle,
    ) -> Result<&'a mut RedisConn, DriverError> {
        conns
            .get_mut(&handle.pool_id)
            .ok_or_else(|| DriverError::ConnectionFailed("Redis connection not found".into()))
    }

    async fn select_db(live: &mut RedisLiveConn, db_index: u32) -> Result<(), String> {
        with_redis_conn!(live, |conn| select_db_on(conn, db_index).await)
    }

    /// Parse a logical database name (`db0`, `db7`) or a bare number into a Redis DB index.
    pub fn parse_db_name(database: &str) -> Result<u32, DriverError> {
        let s = database.trim();
        if s.is_empty() {
            return Err(DriverError::QueryFailed("empty database name".into()));
        }
        if let Some(rest) = s.strip_prefix("db") {
            rest.parse().map_err(|_| {
                DriverError::QueryFailed("invalid database name (expected e.g. db0)".into())
            })
        } else {
            s.parse().map_err(|_| {
                DriverError::QueryFailed("invalid database name (expected e.g. db0)".into())
            })
        }
    }

    /// Scan Redis keys with type, TTL, size, and a short preview. Returns `(next_cursor, entries, dbsize)`.
    pub async fn scan_keys_with_info(
        &self,
        handle: &ConnectionHandle,
        db_index: u32,
        pattern: &str,
        cursor: u64,
        count: u32,
    ) -> Result<(u64, Vec<KeyEntry>, u64), DriverError> {
        let t0 = std::time::Instant::now();
        tracing::info!(db_index, %pattern, cursor, count, "redis scan_keys_with_info: acquiring lock");
        let mut conns = self.connections.write().await;
        tracing::info!(
            lock_ms = t0.elapsed().as_millis() as u64,
            "redis scan_keys_with_info: lock acquired"
        );
        let rc = Self::get_conn(&mut conns, handle)?;
        Self::select_db(&mut rc.live, db_index)
            .await
            .map_err(DriverError::QueryFailed)?;
        let pattern = pattern.to_string();
        with_redis_conn!(&mut rc.live, |conn| scan_keys_with_info_on(
            conn, db_index, &pattern, cursor, count, t0
        )
        .await)
    }

    /// Load the full value for a key in `db_index`.
    pub async fn get_key_detail(
        &self,
        handle: &ConnectionHandle,
        db_index: u32,
        key: &str,
    ) -> Result<KeyDetail, DriverError> {
        let t0 = std::time::Instant::now();
        tracing::info!(db_index, %key, "redis get_key_detail: acquiring lock");
        let mut conns = self.connections.write().await;
        tracing::info!(
            lock_ms = t0.elapsed().as_millis() as u64,
            "redis get_key_detail: lock acquired"
        );
        let rc = Self::get_conn(&mut conns, handle)?;
        Self::select_db(&mut rc.live, db_index)
            .await
            .map_err(DriverError::QueryFailed)?;
        let key = key.to_string();
        with_redis_conn!(&mut rc.live, |conn| get_key_detail_on(conn, &key).await)
    }

    plugin_on_db!(plugin_set_string, (key: &str, value: &str) -> (), |conn| crate::ops::set_string(conn, key, value));
    plugin_on_db!(plugin_hash_set, (key: &str, field: &str, value: &str) -> (), |conn| crate::ops::hash_set(conn, key, field, value));
    plugin_on_db!(plugin_hash_del, (key: &str, fields: &[String]) -> (), |conn| crate::ops::hash_del(conn, key, fields));
    plugin_on_db!(plugin_list_push, (key: &str, side: &str, values: &[String]) -> (), |conn| crate::ops::list_push(conn, key, side, values));
    plugin_on_db!(plugin_list_set, (key: &str, index: i64, value: &str) -> (), |conn| crate::ops::list_set(conn, key, index, value));
    plugin_on_db!(plugin_list_pop, (key: &str, side: &str) -> Option<String>, |conn| crate::ops::list_pop(conn, key, side));
    plugin_on_db!(plugin_set_add, (key: &str, members: &[String]) -> (), |conn| crate::ops::set_add(conn, key, members));
    plugin_on_db!(plugin_set_remove, (key: &str, members: &[String]) -> (), |conn| crate::ops::set_remove(conn, key, members));
    plugin_on_db!(plugin_zset_add, (key: &str, members: &[crate::ops::ZsetMember]) -> (), |conn| crate::ops::zset_add(conn, key, members));
    plugin_on_db!(plugin_zset_remove, (key: &str, members: &[String]) -> (), |conn| crate::ops::zset_remove(conn, key, members));
    plugin_on_db!(plugin_delete_keys, (keys: &[String]) -> u64, |conn| crate::ops::delete_keys(conn, keys));
    plugin_on_db!(plugin_rename_key, (key: &str, new_key: &str) -> (), |conn| crate::ops::rename_key(conn, key, new_key));
    plugin_on_db!(plugin_set_ttl, (key: &str, ttl_seconds: i64) -> (), |conn| crate::ops::set_ttl(conn, key, ttl_seconds));
    plugin_on_db!(plugin_batch_delete_pattern, (pattern: &str) -> crate::ops::BatchDeleteResult, |conn| crate::ops::batch_delete_pattern(conn, pattern));
    plugin_on_db!(plugin_batch_set_ttl, (keys: &[String], ttl_seconds: i64) -> crate::ops::BatchSetTtlResult, |conn| crate::ops::batch_set_ttl(conn, keys, ttl_seconds));
    plugin_on_db!(plugin_count_matching, (pattern: &str) -> u64, |conn| crate::ops::count_matching(conn, pattern));
    plugin_on_db!(plugin_flush_db, () -> (), |conn| crate::ops::flush_db(conn));

    pub async fn plugin_batch_rename_prefix(
        &self,
        connection_id: &str,
        db_index: u32,
        old_prefix: &str,
        new_prefix: &str,
        keys: Option<Vec<String>>,
    ) -> Result<crate::ops::BatchRenameResult, DriverError> {
        let old_prefix = old_prefix.to_string();
        let new_prefix = new_prefix.to_string();
        with_live_op!(self, connection_id, db_index, |conn| {
            crate::ops::batch_rename_prefix(conn, &old_prefix, &new_prefix, keys.clone()).await
        })
    }

    pub async fn plugin_flush_all(&self, connection_id: &str) -> Result<(), DriverError> {
        with_live_any_op!(self, connection_id, |conn| crate::ops::flush_all(conn)
            .await)
    }

    pub async fn plugin_info(
        &self,
        connection_id: &str,
        section: Option<String>,
        node_addr: Option<String>,
    ) -> Result<String, DriverError> {
        if let Some(addr) = node_addr.filter(|s| !s.trim().is_empty()) {
            let plan = self.connection_plan(connection_id).await?;
            let mut conn = open_pinned_node_conn(&plan, addr.trim()).await?;
            return crate::ops_observe::fetch_info(&mut conn, section.as_deref())
                .await
                .map_err(DriverError::QueryFailed);
        }
        with_live_any_op!(self, connection_id, |conn| {
            crate::ops_observe::fetch_info(conn, section.as_deref()).await
        })
    }

    pub async fn plugin_cluster_nodes(
        &self,
        connection_id: &str,
    ) -> Result<crate::ops_cluster::ClusterNodesResult, DriverError> {
        with_live_any_op!(self, connection_id, |conn| {
            crate::ops_cluster::cluster_nodes(conn).await
        })
    }

    pub async fn plugin_memory_sample(
        &self,
        connection_id: &str,
        db_index: u32,
        limit: Option<u32>,
    ) -> Result<crate::ops_observe::MemorySampleResult, DriverError> {
        let limit = crate::ops_observe::resolve_memory_sample_limit(limit);
        with_live_op!(self, connection_id, db_index, |conn| {
            crate::ops_observe::memory_sample(conn, limit).await
        })
    }

    pub async fn plugin_slowlog_get(
        &self,
        connection_id: &str,
        count: u32,
    ) -> Result<Vec<crate::ops_observe::SlowlogEntry>, DriverError> {
        with_live_any_op!(self, connection_id, |conn| {
            crate::ops_observe::slowlog_get(conn, count).await
        })
    }

    pub async fn plugin_slowlog_reset(&self, connection_id: &str) -> Result<(), DriverError> {
        with_live_any_op!(self, connection_id, |conn| {
            crate::ops_observe::slowlog_reset(conn).await
        })
    }

    pub async fn plugin_modules_list(
        &self,
        connection_id: &str,
    ) -> Result<Vec<String>, DriverError> {
        with_live_any_op!(self, connection_id, |conn| {
            crate::ops_observe::modules_list(conn).await
        })
    }

    pub async fn plugin_exec(
        &self,
        connection_id: &str,
        db_index: u32,
        commands: &str,
        node_addr: Option<String>,
    ) -> Result<crate::ops_exec::ExecResponse, DriverError> {
        let commands = commands.to_string();
        if let Some(addr) = node_addr.filter(|s| !s.trim().is_empty()) {
            let plan = self.connection_plan(connection_id).await?;
            let mut conn = open_pinned_node_conn(&plan, addr.trim()).await?;
            select_db_on(&mut conn, db_index)
                .await
                .map_err(DriverError::QueryFailed)?;
            return crate::ops_exec::exec_commands(&mut conn, &commands)
                .await
                .map_err(DriverError::QueryFailed);
        }
        with_live_op!(self, connection_id, db_index, |conn| {
            crate::ops_exec::exec_commands(conn, &commands).await
        })
    }

    pub async fn connection_plan(
        &self,
        connection_id: &str,
    ) -> Result<ConnectionPlan, DriverError> {
        let conns = self.connections.read().await;
        conns
            .get(connection_id)
            .map(|rc| rc.plan.clone())
            .ok_or_else(|| DriverError::ConnectionFailed("Redis connection not found".into()))
    }

    pub async fn plugin_pubsub_publish(
        &self,
        connection_id: &str,
        channel: &str,
        message: &str,
    ) -> Result<u64, DriverError> {
        let channel = channel.to_string();
        let message = message.to_string();
        with_live_any_op!(self, connection_id, |conn| {
            crate::ops_pubsub::publish(conn, &channel, &message).await
        })
    }

    plugin_on_db!(plugin_json_get, (key: &str, path: &str) -> crate::ops_json::JsonGetResult, |conn| crate::ops_json::json_get(conn, key, path));
    plugin_on_db!(plugin_json_set, (key: &str, path: &str, value: &str) -> (), |conn| crate::ops_json::json_set(conn, key, path, value));
    plugin_on_db!(plugin_json_del, (key: &str, path: &str) -> crate::ops_json::JsonDelResult, |conn| crate::ops_json::json_del(conn, key, path));

    plugin_on_db!(
        plugin_xrange,
        (key: &str, start: &str, end: &str, count: Option<u32>) -> crate::ops_stream::XrangeResult,
        |conn| crate::ops_stream::xrange(conn, key, start, end, count)
    );

    pub async fn plugin_xadd(
        &self,
        connection_id: &str,
        db_index: u32,
        key: &str,
        fields: &std::collections::HashMap<String, String>,
        id: Option<String>,
    ) -> Result<crate::ops_stream::XaddResult, DriverError> {
        let key = key.to_string();
        let fields = fields.clone();
        with_live_op!(self, connection_id, db_index, |conn| {
            crate::ops_stream::xadd(conn, &key, &fields, id.as_deref()).await
        })
    }

    pub async fn plugin_xgroup_create(
        &self,
        connection_id: &str,
        db_index: u32,
        key: &str,
        group: &str,
        start_id: Option<String>,
    ) -> Result<(), DriverError> {
        let key = key.to_string();
        let group = group.to_string();
        with_live_op!(self, connection_id, db_index, |conn| {
            crate::ops_stream::xgroup_create(conn, &key, &group, start_id.as_deref()).await
        })
    }

    plugin_on_db!(
        plugin_xgroup_destroy,
        (key: &str, group: &str) -> (),
        |conn| crate::ops_stream::xgroup_destroy(conn, key, group)
    );

    plugin_on_db!(
        plugin_xinfo_groups,
        (key: &str) -> Vec<crate::ops_stream::StreamGroupInfo>,
        |conn| crate::ops_stream::xinfo_groups(conn, key)
    );

    pub async fn plugin_xpending(
        &self,
        connection_id: &str,
        db_index: u32,
        key: &str,
        group: &str,
        start: Option<String>,
        end: Option<String>,
        count: Option<u32>,
        consumer: Option<String>,
    ) -> Result<crate::ops_stream::XpendingResult, DriverError> {
        let key = key.to_string();
        let group = group.to_string();
        with_live_op!(self, connection_id, db_index, |conn| {
            crate::ops_stream::xpending(
                conn,
                &key,
                &group,
                start.as_deref(),
                end.as_deref(),
                count,
                consumer.as_deref(),
            )
            .await
        })
    }

    pub async fn plugin_xack(
        &self,
        connection_id: &str,
        db_index: u32,
        key: &str,
        group: &str,
        ids: &[String],
    ) -> Result<u64, DriverError> {
        let key = key.to_string();
        let group = group.to_string();
        let ids = ids.to_vec();
        with_live_op!(self, connection_id, db_index, |conn| {
            crate::ops_stream::xack(conn, &key, &group, &ids).await
        })
    }

    pub async fn plugin_stream_overview(
        &self,
        connection_id: &str,
        db_index: u32,
        limit: Option<u32>,
    ) -> Result<crate::ops_stream::StreamOverviewResult, DriverError> {
        let limit = crate::ops_stream::resolve_stream_overview_limit(limit);
        with_live_op!(self, connection_id, db_index, |conn| {
            crate::ops_stream::stream_overview(conn, limit).await
        })
    }

    pub async fn plugin_dump_keys(
        &self,
        connection_id: &str,
        db_index: u32,
        keys: &[String],
    ) -> Result<crate::ops_io::DumpKeysResult, DriverError> {
        let keys = keys.to_vec();
        with_live_op!(self, connection_id, db_index, |conn| {
            crate::ops_io::dump_keys(conn, &keys).await
        })
    }

    pub async fn plugin_restore_keys(
        &self,
        connection_id: &str,
        db_index: u32,
        entries: Vec<crate::ops_io::RestoreKeyEntry>,
        replace: bool,
    ) -> Result<crate::ops_io::RestoreKeysResult, DriverError> {
        with_live_op!(self, connection_id, db_index, |conn| {
            crate::ops_io::restore_keys(conn, &entries, replace).await
        })
    }
}

async fn select_db_on<C>(conn: &mut C, db_index: u32) -> Result<(), String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    redis::cmd("SELECT")
        .arg(db_index)
        .query_async(conn)
        .await
        .map_err(|e| e.to_string())
}

async fn info_server_on<C>(conn: &mut C) -> Result<String, String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    redis::cmd("INFO")
        .arg("server")
        .query_async(conn)
        .await
        .map_err(|e| e.to_string())
}

async fn query_cmd_on<C>(
    conn: &mut C,
    cmd_name: &str,
    cmd_args: &[String],
) -> Result<redis::Value, DriverError>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    let mut cmd = redis::cmd(cmd_name);
    for part in cmd_args {
        cmd.arg(part.as_str());
    }
    cmd.query_async(conn)
        .await
        .map_err(|e| DriverError::QueryFailed(e.to_string()))
}

async fn get_tables_on<C>(
    conn: &mut C,
    database: &str,
    t0: Instant,
) -> Result<Vec<TableInfo>, DriverError>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    const MAX_KEYS: usize = 10_000;
    let mut keys = crate::ops::scan_keys(conn, None, Some(MAX_KEYS))
        .await
        .map_err(DriverError::QueryFailed)?;
    tracing::info!(%database, key_count = keys.len(), ms = t0.elapsed().as_millis() as u64, "redis get_tables: scan done");
    keys.sort();
    Ok(keys
        .into_iter()
        .map(|key| TableInfo {
            name: key,
            schema: None,
            table_type: TableType::Table,
            row_count: None,
        })
        .collect())
}

async fn scan_keys_with_info_on<C>(
    conn: &mut C,
    db_index: u32,
    pattern: &str,
    cursor: u64,
    count: u32,
    t0: Instant,
) -> Result<(u64, Vec<KeyEntry>, u64), DriverError>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    let db_size: u64 = redis::cmd("DBSIZE")
        .query_async(conn)
        .await
        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
    tracing::info!(
        db_index,
        db_size,
        ms = t0.elapsed().as_millis() as u64,
        "redis scan_keys_with_info: SELECT+DBSIZE done"
    );

    let scan_raw: redis::Value = redis::cmd("SCAN")
        .arg(cursor)
        .arg("MATCH")
        .arg(pattern)
        .arg("COUNT")
        .arg(count.max(1))
        .query_async(conn)
        .await
        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
    let (next_cursor, key_names) = parse_scan_result(&scan_raw);

    if key_names.is_empty() {
        return Ok((next_cursor, vec![], db_size));
    }

    let mut pipe1 = redis::pipe();
    for k in &key_names {
        pipe1.cmd("TYPE").arg(k);
        pipe1.cmd("TTL").arg(k);
    }
    let r1: Vec<redis::Value> = pipe1
        .query_async(conn)
        .await
        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

    let mut types: Vec<String> = Vec::with_capacity(key_names.len());
    let mut ttls: Vec<i64> = Vec::with_capacity(key_names.len());
    for i in 0..key_names.len() {
        let tval = r1
            .get(2 * i)
            .ok_or_else(|| DriverError::QueryFailed("TYPE pipeline: missing value".into()))?;
        let ttlval = r1
            .get(2 * i + 1)
            .ok_or_else(|| DriverError::QueryFailed("TTL pipeline: missing value".into()))?;
        types.push(value_to_type_string(tval));
        ttls.push(
            i64::from_redis_value(ttlval)
                .map_err(|e| DriverError::QueryFailed(format!("TTL: {e}")))?,
        );
    }

    let mut pipe2 = redis::pipe();
    for (k, tk) in key_names.iter().zip(&types) {
        match tk.as_str() {
            "string" => {
                pipe2.cmd("STRLEN").arg(k);
            }
            "hash" => {
                pipe2.cmd("HLEN").arg(k);
            }
            "list" => {
                pipe2.cmd("LLEN").arg(k);
            }
            "set" => {
                pipe2.cmd("SCARD").arg(k);
            }
            "zset" => {
                pipe2.cmd("ZCARD").arg(k);
            }
            "stream" => {
                pipe2.cmd("XLEN").arg(k);
            }
            _ => {
                pipe2.cmd("PING");
            }
        }
    }
    let r2: Vec<redis::Value> = pipe2
        .query_async(conn)
        .await
        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

    let mut pipe3 = redis::pipe();
    for (k, tk) in key_names.iter().zip(&types) {
        match tk.as_str() {
            "string" => {
                pipe3.cmd("GETRANGE").arg(k).arg(0i64).arg(255i64);
            }
            "hash" => {
                pipe3.cmd("HSCAN").arg(k).arg(0i64).arg("COUNT").arg(3i64);
            }
            "list" => {
                pipe3.cmd("LINDEX").arg(k).arg(0i64);
            }
            "set" => {
                pipe3.cmd("SRANDMEMBER").arg(k);
            }
            "zset" => {
                pipe3
                    .cmd("ZRANGE")
                    .arg(k)
                    .arg(0i64)
                    .arg(0i64)
                    .arg("WITHSCORES");
            }
            "stream" => {
                pipe3
                    .cmd("XREVRANGE")
                    .arg(k)
                    .arg("+")
                    .arg("-")
                    .arg("COUNT")
                    .arg(1i64);
            }
            _ => {
                pipe3.cmd("PING");
            }
        }
    }
    let r3: Vec<redis::Value> = pipe3
        .query_async(conn)
        .await
        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

    let mut keys = Vec::with_capacity(key_names.len());
    for i in 0..key_names.len() {
        let tk = &types[i];
        let size = if matches!(tk.as_str(), "none") {
            0u64
        } else {
            value_to_u64(
                r2.get(i)
                    .ok_or_else(|| DriverError::QueryFailed("size pipeline".into()))?,
            )
        };
        let preview = if tk == "none" {
            String::new()
        } else {
            preview_value_to_string(
                r3.get(i)
                    .ok_or_else(|| DriverError::QueryFailed("preview pipeline".into()))?,
                tk,
            )
        };
        keys.push(KeyEntry {
            key: key_names[i].clone(),
            key_type: tk.clone(),
            ttl: ttls[i],
            size,
            preview: truncate_preview(&preview, 512),
        });
    }

    Ok((next_cursor, keys, db_size))
}

async fn get_key_detail_on<C>(conn: &mut C, key: &str) -> Result<KeyDetail, DriverError>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    let key_type: String = conn
        .key_type(key)
        .await
        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
    let ttl: i64 = conn
        .ttl(key)
        .await
        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
    if key_type == "none" {
        return Err(DriverError::QueryFailed("Key does not exist".into()));
    }

    let value = match key_type.as_str() {
        "string" => {
            let raw: redis::Value = redis::cmd("GET")
                .arg(key)
                .query_async(conn)
                .await
                .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
            serde_json::json!({ "value": value_to_string(&raw) })
        }
        "hash" => {
            let raw: redis::Value = redis::cmd("HGETALL")
                .arg(key)
                .query_async(conn)
                .await
                .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
            serde_json::json!({ "fields": serde_json::Value::Object(redis_flat_pairs_to_map(&raw)) })
        }
        "list" => {
            let raw: redis::Value = redis::cmd("LRANGE")
                .arg(key)
                .arg(0i64)
                .arg(-1i64)
                .query_async(conn)
                .await
                .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
            serde_json::json!({ "items": redis_array_to_json_strings(&raw) })
        }
        "set" => {
            let raw: redis::Value = redis::cmd("SMEMBERS")
                .arg(key)
                .query_async(conn)
                .await
                .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
            serde_json::json!({ "members": redis_array_to_json_strings(&raw) })
        }
        "zset" => {
            let raw: redis::Value = redis::cmd("ZRANGE")
                .arg(key)
                .arg(0i64)
                .arg(-1i64)
                .arg("WITHSCORES")
                .query_async(conn)
                .await
                .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
            serde_json::json!({ "members": redis_zset_to_json(&raw) })
        }
        "stream" => {
            let len: u64 = redis::cmd("XLEN")
                .arg(key)
                .query_async(conn)
                .await
                .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
            let raw: Vec<redis::Value> = redis::cmd("XRANGE")
                .arg(key)
                .arg("-")
                .arg("+")
                .arg("COUNT")
                .arg(10_000i64)
                .query_async(conn)
                .await
                .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
            let entries: Vec<serde_json::Value> = raw
                .iter()
                .filter_map(|v| stream_entry_to_json(v).ok())
                .collect();
            serde_json::json!({ "length": len, "entries": entries })
        }
        _ => {
            let u: String = conn
                .key_type(key)
                .await
                .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
            serde_json::json!({ "raw": format!("(unsupported or module type) {u}") })
        }
    };

    Ok(KeyDetail {
        key: key.to_string(),
        key_type,
        ttl,
        value,
    })
}

/// Safely extract a string from a redis::Value, handling non-UTF-8 bytes via lossy conversion.
fn value_to_string(v: &redis::Value) -> String {
    match v {
        redis::Value::BulkString(b) => String::from_utf8_lossy(b).to_string(),
        redis::Value::SimpleString(s) => s.clone(),
        redis::Value::VerbatimString { text, .. } => text.clone(),
        redis::Value::Int(i) => i.to_string(),
        redis::Value::Okay => "OK".into(),
        _ => format!("{:?}", v),
    }
}

/// Parse a SCAN result (Array of [cursor, Array of keys]) into (next_cursor, Vec<String>).
/// Tolerates non-UTF-8 keys by using lossy conversion.
pub(crate) fn parse_scan_result(v: &redis::Value) -> (u64, Vec<String>) {
    match v {
        redis::Value::Array(items) if items.len() >= 2 => {
            let next_cursor: u64 = match &items[0] {
                redis::Value::BulkString(b) => String::from_utf8_lossy(b).parse().unwrap_or(0),
                redis::Value::Int(i) => *i as u64,
                _ => 0,
            };
            let keys = match &items[1] {
                redis::Value::Array(arr) => arr.iter().map(|v| value_to_string(v)).collect(),
                _ => vec![],
            };
            (next_cursor, keys)
        }
        _ => (0, vec![]),
    }
}

fn value_to_type_string(v: &redis::Value) -> String {
    match v {
        redis::Value::BulkString(b) => String::from_utf8_lossy(b).to_lowercase(),
        redis::Value::VerbatimString { text, .. } => text.to_lowercase(),
        redis::Value::Int(i) => i.to_string(),
        redis::Value::SimpleString(s) => s.to_lowercase(),
        redis::Value::Okay => "ok".into(),
        _ => "unknown".into(),
    }
}

fn value_to_u64(v: &redis::Value) -> u64 {
    match v {
        redis::Value::Int(i) => *i as u64,
        redis::Value::BulkString(b) => String::from_utf8_lossy(b).parse().unwrap_or(0),
        _ => 0,
    }
}

fn preview_value_to_string(v: &redis::Value, key_type: &str) -> String {
    if key_type == "zset" {
        if let Ok(parts) = Vec::<String>::from_redis_value(v) {
            if !parts.is_empty() {
                let member = &parts[0];
                return if parts.len() >= 2 {
                    format!("{member} (score: {})", parts[1])
                } else {
                    member.clone()
                };
            }
        }
    }
    if key_type == "stream" {
        if let Some(s) = stream_preview_from_xrev(v) {
            return s;
        }
    }
    match v {
        redis::Value::Nil => String::new(),
        redis::Value::Array(a) if a.is_empty() => String::new(),
        redis::Value::BulkString(b) => String::from_utf8_lossy(b).to_string(),
        redis::Value::VerbatimString { text, .. } => text.clone(),
        redis::Value::Int(i) => i.to_string(),
        redis::Value::Map(pairs) if key_type == "hash" => {
            let n = 2.min(pairs.len());
            let mut s = "(".to_string();
            for (i, (fk, fv)) in pairs.iter().take(n).enumerate() {
                if i > 0 {
                    s.push_str(", ");
                }
                s.push_str(&value_field_for_preview(fk));
                s.push_str(": ");
                s.push_str(&value_field_for_preview(fv));
            }
            s.push(')');
            if pairs.len() > 2 {
                s.push_str(" …");
            }
            s
        }
        // HSCAN returns [cursor, [field, val, ...]] — unwrap the inner array
        redis::Value::Array(items) if key_type == "hash" && items.len() == 2 => {
            let fields = match &items[1] {
                redis::Value::Array(inner) => inner,
                _ => return format!("{v:?}"),
            };
            if fields.is_empty() {
                return String::new();
            }
            let n = 4.min(fields.len());
            let mut s = "(".to_string();
            for i in (0..n).step_by(2) {
                if i + 1 < fields.len() {
                    let f = value_field_for_preview(&fields[i]);
                    let val = value_field_for_preview(&fields[i + 1]);
                    s.push_str(&f);
                    s.push_str(": ");
                    s.push_str(&val);
                    if i + 2 < n {
                        s.push_str(", ");
                    }
                }
            }
            s.push(')');
            if fields.len() > 4 {
                s.push_str(" …");
            }
            s
        }
        // Legacy HGETALL flat array format: [field, val, field, val, ...]
        redis::Value::Array(items) if key_type == "hash" && !items.is_empty() => {
            let n = 4.min(items.len());
            let mut s = "(".to_string();
            for i in (0..n).step_by(2) {
                if i + 1 < items.len() {
                    let f = value_field_for_preview(&items[i]);
                    let val = value_field_for_preview(&items[i + 1]);
                    s.push_str(&f);
                    s.push_str(": ");
                    s.push_str(&val);
                    if i + 2 < n {
                        s.push_str(", ");
                    }
                }
            }
            s.push(')');
            if items.len() > 4 {
                s.push_str(" …");
            }
            s
        }
        redis::Value::Array(items) if !items.is_empty() => format!("{items:?}"),
        redis::Value::SimpleString(s) if s == "PONG" => String::new(),
        redis::Value::Okay => String::new(),
        _ => format!("{v:?}"),
    }
}

fn value_field_for_preview(v: &redis::Value) -> String {
    match v {
        redis::Value::BulkString(b) => String::from_utf8_lossy(b).to_string(),
        redis::Value::VerbatimString { text, .. } => text.clone(),
        redis::Value::Int(i) => i.to_string(),
        redis::Value::SimpleString(s) => s.clone(),
        redis::Value::Okay => "OK".into(),
        _ => format!("{v:?}"),
    }
}

/// Convert a flat Redis array `[field, val, field, val, ...]` or Map to a JSON object.
fn redis_flat_pairs_to_map(v: &redis::Value) -> serde_json::Map<String, serde_json::Value> {
    let mut obj = serde_json::Map::new();
    match v {
        redis::Value::Array(items) => {
            for chunk in items.chunks(2) {
                if chunk.len() == 2 {
                    let k = value_to_string(&chunk[0]);
                    let val = value_to_string(&chunk[1]);
                    obj.insert(k, serde_json::Value::String(val));
                }
            }
        }
        redis::Value::Map(pairs) => {
            for (fk, fv) in pairs {
                let k = value_to_string(fk);
                let val = value_to_string(fv);
                obj.insert(k, serde_json::Value::String(val));
            }
        }
        _ => {}
    }
    obj
}

/// Convert a Redis array to a Vec of JSON strings (lossy UTF-8).
fn redis_array_to_json_strings(v: &redis::Value) -> Vec<serde_json::Value> {
    match v {
        redis::Value::Array(items) => items
            .iter()
            .map(|item| serde_json::Value::String(value_to_string(item)))
            .collect(),
        _ => vec![],
    }
}

/// Convert a Redis ZRANGE ... WITHSCORES flat array to `[{"member":..,"score":..}, ...]`.
fn redis_zset_to_json(v: &redis::Value) -> Vec<serde_json::Value> {
    let mut members = Vec::new();
    if let redis::Value::Array(items) = v {
        for chunk in items.chunks(2) {
            if chunk.len() == 2 {
                let mem = value_to_string(&chunk[0]);
                let sc: f64 = value_to_string(&chunk[1]).parse().unwrap_or(0.0);
                members.push(serde_json::json!({ "member": mem, "score": sc }));
            }
        }
    }
    members
}

/// `XREVRANGE` with COUNT 1: `[[id, [field, val, ...]]]`
fn stream_preview_from_xrev(v: &redis::Value) -> Option<String> {
    let a = match v {
        redis::Value::Array(x) => x,
        _ => return None,
    };
    if a.is_empty() {
        return Some(String::new());
    }
    let id = value_field_for_preview(&a[0]);
    if a.len() < 2 {
        return Some(id);
    }
    let rest = match &a[1] {
        redis::Value::Array(fields) if !fields.is_empty() => {
            let mut s = id + ": ";
            for f in fields.iter().take(2) {
                s.push_str(&value_field_for_preview(f));
            }
            s
        }
        other => format!("{id}: {other:?}"),
    };
    Some(rest)
}

fn stream_entry_to_json(v: &redis::Value) -> Result<serde_json::Value, ()> {
    let a = match v {
        redis::Value::Array(x) if !x.is_empty() => x,
        _ => return Err(()),
    };
    let id = value_field_for_preview(&a[0]);
    if a.len() < 2 {
        return Ok(serde_json::json!({ "id": id, "fields": {} }));
    }
    let mut map = serde_json::Map::new();
    if let redis::Value::Array(fields) = &a[1] {
        for pair in fields.chunks(2) {
            if pair.len() == 2 {
                let k = value_field_for_preview(&pair[0]);
                let val = value_field_for_preview(&pair[1]);
                map.insert(k, serde_json::Value::String(val));
            }
        }
    }
    Ok(serde_json::json!({ "id": id, "fields": serde_json::Value::Object(map) }))
}

fn truncate_preview(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max).collect::<String>() + "…"
    }
}

/// Split a Redis command line, respecting double-quoted arguments (spaces inside quotes).
pub(crate) fn parse_redis_command_args(s: &str) -> Result<Vec<String>, DriverError> {
    let s = s.trim();
    if s.is_empty() {
        return Err(DriverError::QueryFailed("Empty command".into()));
    }
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        if bytes[i] == b'"' {
            i += 1;
            let mut cur = String::new();
            while i < bytes.len() {
                if bytes[i] == b'\\' && i + 1 < bytes.len() {
                    i += 1;
                    cur.push(bytes[i] as char);
                    i += 1;
                } else if bytes[i] == b'"' {
                    i += 1;
                    break;
                } else {
                    cur.push(bytes[i] as char);
                    i += 1;
                }
            }
            out.push(cur);
        } else {
            let start = i;
            while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            out.push(s[start..i].to_string());
        }
    }
    if out.is_empty() {
        return Err(DriverError::QueryFailed("Empty command".into()));
    }
    Ok(out)
}

#[async_trait]
impl DatabaseDriver for RedisDriver {
    fn driver_type(&self) -> DatabaseType {
        "redis".to_string()
    }

    fn driver_category(&self) -> DriverCategory {
        DriverCategory::KeyValue
    }

    fn quote_char(&self) -> char {
        '\0' // Redis doesn't quote identifiers
    }

    fn quote_ident(&self, name: &str) -> String {
        name.to_string()
    }

    async fn test_connection(&self, config: &ConnectionConfig) -> Result<ServerInfo, DriverError> {
        let plan = build_connection_plan(config)?;
        let mut live = open_live_conn(&plan).await?;
        if let ConnectionPlan::Standalone(p) = &plan {
            Self::select_db(&mut live, p.db_index)
                .await
                .map_err(DriverError::QueryFailed)?;
        } else if let ConnectionPlan::Sentinel(p) = &plan {
            Self::select_db(&mut live, p.db_index)
                .await
                .map_err(DriverError::QueryFailed)?;
        }

        let info: String = with_redis_conn!(&mut live, |conn| info_server_on(conn).await)
            .map_err(DriverError::QueryFailed)?;

        let version = info
            .lines()
            .find(|l| l.starts_with("redis_version:"))
            .map(|l| l.trim_start_matches("redis_version:").trim().to_string())
            .unwrap_or_else(|| "unknown".into());

        Ok(ServerInfo {
            server_version: version,
            server_type: "Redis".into(),
        })
    }

    async fn connect(&self, config: &ConnectionConfig) -> Result<ConnectionHandle, DriverError> {
        let plan = build_connection_plan(config)?;
        let pool_id = format!("redis_{}", uuid::Uuid::new_v4());
        let live = open_live_conn(&plan).await?;

        let mut conns = self.connections.write().await;
        conns.insert(pool_id.clone(), RedisConn { plan, live });

        Ok(ConnectionHandle {
            id: pool_id.clone(),
            pool_id,
        })
    }

    async fn disconnect(&self, handle: ConnectionHandle) -> Result<(), DriverError> {
        let t0 = std::time::Instant::now();
        tracing::info!(pool_id = %handle.pool_id, "redis disconnect: acquiring lock");
        let mut conns = self.connections.write().await;
        tracing::info!(
            lock_ms = t0.elapsed().as_millis() as u64,
            "redis disconnect: lock acquired"
        );
        conns.remove(&handle.pool_id);
        tracing::info!("redis disconnect: done");
        Ok(())
    }

    async fn get_databases(&self, handle: &ConnectionHandle) -> Result<Vec<String>, DriverError> {
        let t0 = std::time::Instant::now();
        tracing::info!("redis get_databases: acquiring lock");
        let mut conns = self.connections.write().await;
        tracing::info!(
            lock_ms = t0.elapsed().as_millis() as u64,
            "redis get_databases: lock acquired"
        );
        let rc = Self::get_conn(&mut conns, handle)?;

        // Query CONFIG GET databases to find the total number of databases,
        // then return all db0..dbN-1 regardless of whether they have keys.
        let db_count: u32 = match with_redis_conn!(&mut rc.live, |conn| {
            redis::cmd("CONFIG")
                .arg("GET")
                .arg("databases")
                .query_async::<redis::Value>(conn)
                .await
        }) {
            Ok(val) => {
                let s = value_to_string(&val);
                s.lines()
                    .filter_map(|l| l.trim().parse::<u32>().ok())
                    .next()
                    .unwrap_or(16)
            }
            Err(_) => 16,
        };

        let out: Vec<String> = (0..db_count).map(|i| format!("db{i}")).collect();
        tracing::info!(
            count = out.len(),
            ms = t0.elapsed().as_millis() as u64,
            "redis get_databases: done"
        );
        Ok(out)
    }

    async fn get_tables(
        &self,
        handle: &ConnectionHandle,
        database: &str,
    ) -> Result<Vec<TableInfo>, DriverError> {
        let t0 = std::time::Instant::now();
        let db_index = Self::parse_db_name(database)?;
        // Read the key list on a short-lived dedicated connection so the
        // shared session's selected DB stays untouched. Callers (e.g. the
        // sidebar cache refresh) enumerate several databases in a row and
        // must never flip the session as a side effect.
        let plan = {
            let mut conns = self.connections.write().await;
            let rc = Self::get_conn(&mut conns, handle)?;
            rc.plan.clone()
        };
        let mut live = open_live_conn(&plan).await?;
        Self::select_db(&mut live, db_index)
            .await
            .map_err(DriverError::QueryFailed)?;
        let database = database.to_string();
        with_redis_conn!(&mut live, |conn| get_tables_on(conn, &database, t0).await)
    }

    async fn get_table_schema(
        &self,
        handle: &ConnectionHandle,
        table: &str,
    ) -> Result<TableSchema, DriverError> {
        let mut conns = self.connections.write().await;
        let rc = Self::get_conn(&mut conns, handle)?;

        let table_name = table.to_string();
        let key_type: String = with_redis_conn!(&mut rc.live, |conn| {
            conn.key_type(&table_name)
                .await
                .map_err(|e| DriverError::QueryFailed(e.to_string()))
        })?;

        let columns = match key_type.as_str() {
            "hash" => vec![
                ColumnSchema {
                    name: "field".into(),
                    data_type: "string".into(),
                    nullable: false,
                    default_value: None,
                    is_primary_key: true,
                    is_auto_increment: false,
                    comment: None,
                },
                ColumnSchema {
                    name: "value".into(),
                    data_type: "string".into(),
                    nullable: true,
                    default_value: None,
                    is_primary_key: false,
                    is_auto_increment: false,
                    comment: None,
                },
            ],
            "list" | "set" | "zset" => {
                let mut cols = vec![ColumnSchema {
                    name: "value".into(),
                    data_type: "string".into(),
                    nullable: false,
                    default_value: None,
                    is_primary_key: false,
                    is_auto_increment: false,
                    comment: None,
                }];
                if key_type == "zset" {
                    cols.push(ColumnSchema {
                        name: "score".into(),
                        data_type: "float".into(),
                        nullable: false,
                        default_value: None,
                        is_primary_key: false,
                        is_auto_increment: false,
                        comment: None,
                    });
                }
                cols
            }
            _ => vec![ColumnSchema {
                name: "value".into(),
                data_type: key_type.clone(),
                nullable: true,
                default_value: None,
                is_primary_key: false,
                is_auto_increment: false,
                comment: None,
            }],
        };

        Ok(TableSchema {
            table_name: table_name.clone(),
            columns,
            primary_keys: vec![],
            indexes: vec![],
            foreign_keys: vec![],
        })
    }

    async fn query(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
    ) -> Result<QueryResult, DriverError> {
        let start = Instant::now();
        let parts = parse_redis_command_args(sql)?;

        tracing::debug!(cmd = %sql, "redis query: acquiring lock");
        let mut conns = self.connections.write().await;
        tracing::debug!(
            lock_ms = start.elapsed().as_millis() as u64,
            "redis query: lock acquired"
        );
        let rc = Self::get_conn(&mut conns, handle)?;

        let cmd_name = parts[0].clone();
        let cmd_args: Vec<String> = parts[1..].to_vec();

        let result: redis::Value = with_redis_conn!(&mut rc.live, |conn| query_cmd_on(
            conn, &cmd_name, &cmd_args
        )
        .await)?;

        let (columns, rows) = redis_value_to_rows(&result);

        Ok(QueryResult {
            columns,
            rows,
            rows_affected: None,
            execution_time_ms: start.elapsed().as_millis() as u64,
        })
    }

    async fn query_multi(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        _limit: Option<u32>,
    ) -> Result<MultiQueryResult, DriverError> {
        let total_start = Instant::now();
        let commands: Vec<&str> = sql
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect();
        let mut results = Vec::new();

        for cmd_str in commands {
            let start = Instant::now();
            match self.query(handle, cmd_str).await {
                Ok(qr) => {
                    results.push(StatementResult {
                        sql: cmd_str.to_string(),
                        columns: qr.columns,
                        rows: qr.rows,
                        rows_affected: qr.rows_affected,
                        execution_time_ms: start.elapsed().as_millis() as u64,
                        truncated: false,
                    });
                }
                Err(e) => {
                    tracing::warn!(cmd = cmd_str, error = %e, "redis query_multi command failed");
                    results.push(StatementResult {
                        sql: cmd_str.to_string(),
                        columns: vec![],
                        rows: vec![],
                        rows_affected: None,
                        execution_time_ms: start.elapsed().as_millis() as u64,
                        truncated: false,
                    });
                }
            }
        }

        Ok(MultiQueryResult {
            results,
            total_time_ms: total_start.elapsed().as_millis() as u64,
        })
    }

    async fn query_stream(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        limit: Option<u32>,
        on_event: QueryStreamCallback,
    ) -> Result<(), DriverError> {
        let total_start = Instant::now();
        let commands: Vec<String> = sql
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if commands.is_empty() {
            on_event(QueryStreamEvent::Done { total_time_ms: 0 });
            return Ok(());
        }
        for (index, cmd_str) in commands.iter().enumerate() {
            let start = Instant::now();
            match self.query(handle, cmd_str).await {
                Ok(mut qr) => {
                    stream_decoded_rows(
                        &on_event,
                        index,
                        cmd_str.clone(),
                        qr.columns,
                        std::mem::take(&mut qr.rows),
                        limit,
                        start.elapsed().as_millis() as u64,
                        qr.rows_affected,
                    );
                }
                Err(e) => {
                    tracing::warn!(cmd = %cmd_str, error = %e, "redis query_stream command failed");
                    stream_decoded_rows(
                        &on_event,
                        index,
                        cmd_str.clone(),
                        vec![],
                        Vec::<Vec<Option<Value>>>::new(),
                        None,
                        start.elapsed().as_millis() as u64,
                        None,
                    );
                }
            }
        }
        on_event(QueryStreamEvent::Done {
            total_time_ms: total_start.elapsed().as_millis() as u64,
        });
        Ok(())
    }

    async fn query_with_params(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        _params: &[Value],
    ) -> Result<QueryResult, DriverError> {
        self.query(handle, sql).await
    }

    async fn execute(&self, handle: &ConnectionHandle, sql: &str) -> Result<u64, DriverError> {
        let result = self.query(handle, sql).await?;
        Ok(result.rows_affected.unwrap_or(0))
    }

    fn command_definitions(&self) -> Vec<datazen_driver_api::DriverCommandDefinition> {
        crate::commands::redis_command_definitions()
    }

    async fn execute_command(
        &self,
        handle: &ConnectionHandle,
        command: &str,
        input: serde_json::Value,
    ) -> Result<datazen_driver_api::CommandResult, DriverError> {
        crate::commands::execute_redis_command(self, handle, command, input).await
    }

    async fn cancel_query(&self, _handle: &ConnectionHandle) -> Result<(), DriverError> {
        tracing::debug!("redis: cancel_query is a no-op (commands are atomic)");
        Ok(())
    }

    async fn get_server_info(&self, handle: &ConnectionHandle) -> Result<ServerInfo, DriverError> {
        let mut conns = self.connections.write().await;
        let redis_conn = Self::get_conn(&mut conns, handle)?;
        let info: String = with_redis_conn!(&mut redis_conn.live, |conn| info_server_on(conn)
            .await)
        .map_err(DriverError::QueryFailed)?;
        let version = info
            .lines()
            .find(|l| l.starts_with("redis_version:"))
            .map(|l| l.trim_start_matches("redis_version:").trim().to_string())
            .unwrap_or_else(|| "unknown".into());
        Ok(ServerInfo {
            server_version: version,
            server_type: "Redis".to_string(),
        })
    }

    async fn dump_database_with_progress(
        &self,
        _handle: &ConnectionHandle,
        _database: &str,
        _opts: &BackupDumpOptions,
        _on_progress: &mut (dyn FnMut(DumpProgress) + Send),
    ) -> Result<String, DriverError> {
        Err(DriverError::NotSupported(
            "Redis does not use SQL dump; export keys via driver commands".into(),
        ))
    }

    async fn restore_sql_with_progress(
        &self,
        _handle: &ConnectionHandle,
        _sql: &str,
        _opts: Option<&BackupRestoreOptions>,
        _on_progress: &mut (dyn FnMut(DumpProgress) + Send),
    ) -> Result<(), DriverError> {
        Err(DriverError::NotSupported(
            "Redis does not restore SQL files; import via driver commands".into(),
        ))
    }
}

#[async_trait]
impl KeyValueDriver for RedisDriver {
    fn driver_type(&self) -> DatabaseType {
        "redis".to_string()
    }

    async fn scan_keys_with_info(
        &self,
        handle: &ConnectionHandle,
        db_index: u32,
        pattern: &str,
        cursor: u64,
        count: u32,
    ) -> Result<(u64, Vec<KeyEntry>, u64), DriverError> {
        RedisDriver::scan_keys_with_info(self, handle, db_index, pattern, cursor, count).await
    }

    async fn get_key_detail(
        &self,
        handle: &ConnectionHandle,
        db_index: u32,
        key: &str,
    ) -> Result<KeyDetail, DriverError> {
        RedisDriver::get_key_detail(self, handle, db_index, key).await
    }
}

/// Convert a Redis value into tabular format for the UI.
fn redis_value_to_rows(value: &redis::Value) -> (Vec<ColumnInfo>, Vec<Vec<Option<Value>>>) {
    match value {
        redis::Value::Nil => (
            vec![ColumnInfo {
                name: "result".into(),
                data_type: "string".into(),
                nullable: true,
            }],
            vec![vec![Some(Value::Null)]],
        ),
        redis::Value::Int(n) => (
            vec![ColumnInfo {
                name: "result".into(),
                data_type: "integer".into(),
                nullable: false,
            }],
            vec![vec![Some(Value::Integer(*n))]],
        ),
        redis::Value::BulkString(bytes) => {
            let s = String::from_utf8_lossy(bytes).to_string();
            (
                vec![ColumnInfo {
                    name: "result".into(),
                    data_type: "string".into(),
                    nullable: false,
                }],
                vec![vec![Some(Value::String(s))]],
            )
        }
        redis::Value::VerbatimString { text, .. } => (
            vec![ColumnInfo {
                name: "result".into(),
                data_type: "string".into(),
                nullable: false,
            }],
            vec![vec![Some(Value::String(text.clone()))]],
        ),
        redis::Value::Array(items) => {
            if items.len() >= 2 && items.len() % 2 == 0 && looks_like_hash(items) {
                let columns = vec![
                    ColumnInfo {
                        name: "field".into(),
                        data_type: "string".into(),
                        nullable: false,
                    },
                    ColumnInfo {
                        name: "value".into(),
                        data_type: "string".into(),
                        nullable: true,
                    },
                ];
                let rows: Vec<Vec<Option<Value>>> = items
                    .chunks(2)
                    .map(|pair| {
                        vec![
                            Some(redis_to_value(&pair[0])),
                            Some(redis_to_value(&pair[1])),
                        ]
                    })
                    .collect();
                (columns, rows)
            } else {
                let columns = vec![
                    ColumnInfo {
                        name: "index".into(),
                        data_type: "integer".into(),
                        nullable: false,
                    },
                    ColumnInfo {
                        name: "value".into(),
                        data_type: "string".into(),
                        nullable: true,
                    },
                ];
                let rows: Vec<Vec<Option<Value>>> = items
                    .iter()
                    .enumerate()
                    .map(|(i, v)| vec![Some(Value::Integer(i as i64)), Some(redis_to_value(v))])
                    .collect();
                (columns, rows)
            }
        }
        redis::Value::SimpleString(s) => (
            vec![ColumnInfo {
                name: "result".into(),
                data_type: "string".into(),
                nullable: false,
            }],
            vec![vec![Some(Value::String(s.clone()))]],
        ),
        #[allow(deprecated)]
        redis::Value::Okay => (
            vec![ColumnInfo {
                name: "result".into(),
                data_type: "string".into(),
                nullable: false,
            }],
            vec![vec![Some(Value::String("OK".into()))]],
        ),
        _ => (
            vec![ColumnInfo {
                name: "result".into(),
                data_type: "string".into(),
                nullable: false,
            }],
            vec![vec![Some(Value::String(format!("{:?}", value)))]],
        ),
    }
}

fn redis_to_value(v: &redis::Value) -> Value {
    match v {
        redis::Value::Nil => Value::Null,
        redis::Value::Int(n) => Value::Integer(*n),
        redis::Value::BulkString(bytes) => {
            Value::String(String::from_utf8_lossy(bytes).to_string())
        }
        redis::Value::VerbatimString { text, .. } => Value::String(text.clone()),
        redis::Value::SimpleString(s) => Value::String(s.clone()),
        #[allow(deprecated)]
        redis::Value::Okay => Value::String("OK".into()),
        redis::Value::Array(items) => {
            let parts: Vec<String> = items.iter().map(|i| format!("{i:?}")).collect();
            Value::String(format!("[{}]", parts.join(", ")))
        }
        _ => Value::String(format!("{v:?}")),
    }
}

fn looks_like_hash(items: &[redis::Value]) -> bool {
    items.chunks(2).all(|pair| {
        matches!(
            &pair[0],
            redis::Value::BulkString(_) | redis::Value::SimpleString(_)
        )
    })
}
