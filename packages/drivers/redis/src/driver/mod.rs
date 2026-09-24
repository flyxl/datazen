//! Redis driver core — session, database, key-value trait impls live here.

mod database;
mod key_value;
pub(crate) mod session;

use datazen_driver_api::*;
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::RwLock;

use crate::types::{KeyDetail, ValueFrame};

use crate::connect::{
    build_connection_plan, looks_like_connection_loss, open_live_conn, open_pinned_node_conn,
    ConnectionPlan, RedisLiveConn,
};
use crate::with_redis_conn;
use session::{get_key_detail_on, get_key_raw_on, info_server_on, select_db_on};

pub(crate) struct RedisConn {
    pub(crate) plan: ConnectionPlan,
    pub(crate) live: RedisLiveConn,
}

/// Like [`with_live_op!`], but hands the operation the connection topology.
///
/// Some batch shapes are only safe on a single-node transport: a cluster
/// connection re-folds per-command errors and pins a pipeline to one slot, so
/// `ops_workbench` has to issue those probes command by command. The topology is
/// read once up front — Sentinel failover swaps the connection but not the
/// variant, so it stays valid across the retry below.
macro_rules! with_live_op_topo {
    ($self:expr, $connection_id:expr, $db_index:expr, |$conn:ident, $topo:ident| $body:expr) => {{
        let handle = ConnectionHandle {
            id: $connection_id.to_string(),
            pool_id: $connection_id.to_string(),
        };
        let mut conns = $self.connections.write().await;
        let rc = RedisDriver::get_conn(&mut conns, &handle)?;
        let $topo = rc.live.topology();
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

macro_rules! with_live_op {
    ($self:expr, $connection_id:expr, $db_index:expr, |$conn:ident| $body:expr) => {{
        with_live_op_topo!($self, $connection_id, $db_index, |$conn, _topology| $body)
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

/// [`plugin_on_db!`] for operations whose request shape depends on the
/// topology; they receive it as the second closure argument.
macro_rules! plugin_on_db_topo {
    ($name:ident, ($($arg:ident: $arg_ty:ty),*) -> $ret:ty, |$conn:ident, $topo:ident| $body:expr) => {
        pub async fn $name(
            &self,
            connection_id: &str,
            db_index: u32,
            $($arg: $arg_ty,)*
        ) -> Result<$ret, DriverError> {
            with_live_op_topo!(self, connection_id, db_index, |$conn, $topo| ($body).await)
        }
    };
}

pub struct RedisDriver {
    pub(crate) connections: RwLock<HashMap<String, RedisConn>>,
}

pub(crate) const TEST_CONNECTION_TLS_GRACE: Duration = Duration::from_secs(5);

impl RedisDriver {
    pub fn new() -> Self {
        Self {
            connections: RwLock::new(HashMap::new()),
        }
    }

    pub(crate) fn get_conn<'a>(
        conns: &'a mut HashMap<String, RedisConn>,
        handle: &ConnectionHandle,
    ) -> Result<&'a mut RedisConn, DriverError> {
        conns
            .get_mut(&handle.pool_id)
            .ok_or_else(|| DriverError::ConnectionFailed("Redis connection not found".into()))
    }

    pub(crate) async fn select_db(live: &mut RedisLiveConn, db_index: u32) -> Result<(), String> {
        with_redis_conn!(live, |conn| select_db_on(conn, db_index).await)
    }

    /// Open a dedicated connection already `SELECT`ed onto `db_index`.
    ///
    /// Metadata reads take the explicit `database` argument as the target DB
    /// index; because Redis has no per-command database selector, that index can
    /// only be honored by `SELECT`. Doing it on a throwaway connection keeps the
    /// shared session's selected database untouched, so browsing one DB never
    /// re-points a read aimed at another.
    pub(crate) async fn open_pinned_conn(
        &self,
        handle: &ConnectionHandle,
        db_index: u32,
    ) -> Result<RedisLiveConn, DriverError> {
        let plan = {
            let mut conns = self.connections.write().await;
            let rc = Self::get_conn(&mut conns, handle)?;
            rc.plan.clone()
        };
        let mut live = open_live_conn(&plan).await?;
        Self::select_db(&mut live, db_index)
            .await
            .map_err(DriverError::QueryFailed)?;
        Ok(live)
    }

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

    /// One page of the flat key browser, under one COUNT budget.
    ///
    /// `budget` is the optional cumulative `COUNT` cap for the *whole* action
    /// (PRD §3.2); `None` derives it from the `DBSIZE` read inside the op. The
    /// topology travels because the per-page attribute batches are one pipeline
    /// on a single node but one addressed batch per key on a cluster.
    #[allow(clippy::too_many_arguments)]
    pub async fn scan_keys_with_info(
        &self,
        handle: &ConnectionHandle,
        db_index: u32,
        pattern: &str,
        cursor: u64,
        count: u32,
        key_type: Option<&str>,
        with_memory: bool,
        no_ttl_only: bool,
        budget: Option<u64>,
    ) -> Result<crate::ops::tree::scan::ScanKeysPage, DriverError> {
        let t0 = std::time::Instant::now();
        tracing::info!(db_index, %pattern, cursor, count, "redis scan_keys_with_info: acquiring lock");
        let mut conns = self.connections.write().await;
        tracing::info!(
            lock_ms = t0.elapsed().as_millis() as u64,
            "redis scan_keys_with_info: lock acquired"
        );
        let rc = Self::get_conn(&mut conns, handle)?;
        let topology = rc.live.topology();
        Self::select_db(&mut rc.live, db_index)
            .await
            .map_err(DriverError::QueryFailed)?;
        let pattern = pattern.to_string();
        let key_type = key_type.map(str::to_string);
        with_redis_conn!(&mut rc.live, |conn| crate::ops::tree::scan::scan_keys_page(
            conn,
            &pattern,
            cursor,
            count,
            key_type.as_deref(),
            with_memory,
            no_ttl_only,
            budget,
            topology,
            t0
        )
        .await)
    }

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

    pub async fn get_key_raw(
        &self,
        handle: &ConnectionHandle,
        db_index: u32,
        key: &str,
        with_memory: bool,
    ) -> Result<ValueFrame, DriverError> {
        let t0 = std::time::Instant::now();
        tracing::info!(db_index, %key, with_memory, "redis get_key_raw: acquiring lock");
        let mut conns = self.connections.write().await;
        tracing::info!(
            lock_ms = t0.elapsed().as_millis() as u64,
            "redis get_key_raw: lock acquired"
        );
        let rc = Self::get_conn(&mut conns, handle)?;
        Self::select_db(&mut rc.live, db_index)
            .await
            .map_err(DriverError::QueryFailed)?;
        let key = key.to_string();
        with_redis_conn!(&mut rc.live, |conn| get_key_raw_on(conn, &key, with_memory)
            .await)
    }

    /// Fetch key counts for every database (SELECT + DBSIZE on a dedicated connection).
    pub async fn db_sizes(
        &self,
        handle: &ConnectionHandle,
    ) -> Result<Vec<serde_json::Value>, DriverError> {
        // 1. Determine db_count from CONFIG GET databases on the shared connection.
        let db_count = {
            let mut conns = self.connections.write().await;
            let rc = Self::get_conn(&mut conns, handle)?;
            with_redis_conn!(&mut rc.live, |conn| {
                redis::cmd("CONFIG")
                    .arg("GET")
                    .arg("databases")
                    .query_async::<redis::Value>(conn)
                    .await
                    .map_err(|e| e.to_string())
            })
            .map_err(DriverError::QueryFailed)
            .and_then(|cfg_val| match cfg_val {
                redis::Value::Array(ref items) if items.len() >= 2 => {
                    let s = match &items[1] {
                        redis::Value::BulkString(b) => String::from_utf8_lossy(b).into_owned(),
                        redis::Value::Int(i) => i.to_string(),
                        _ => "16".to_string(),
                    };
                    Ok(s.parse::<u32>().unwrap_or(16))
                }
                _ => Ok(16),
            })?
        };

        // 2. Get the connection plan for creating dedicated connections.
        let plan = {
            let mut conns = self.connections.write().await;
            let rc = Self::get_conn(&mut conns, handle)?;
            rc.plan.clone()
        };

        // 3. For each database, open a dedicated connection, SELECT + DBSIZE.
        let mut results = Vec::with_capacity(db_count as usize);
        for db in 0..db_count {
            let mut live = open_live_conn(&plan).await?;
            Self::select_db(&mut live, db)
                .await
                .map_err(DriverError::QueryFailed)?;
            let count: u64 = with_redis_conn!(&mut live, |conn| {
                redis::cmd("DBSIZE")
                    .query_async(conn)
                    .await
                    .map_err(|e| e.to_string())
            })
            .map_err(DriverError::QueryFailed)?;
            results.push(serde_json::json!({ "db": db, "keys": count }));
        }
        Ok(results)
    }

    /// List direct children under a key prefix (leaf keys + virtual folders).
    ///
    /// `budget` and `with_memory` behave exactly as in
    /// [`scan_keys_with_info`](Self::scan_keys_with_info): one cumulative COUNT
    /// cap per action, and `MEMORY USAGE` inside the page's meta batch.
    #[allow(clippy::too_many_arguments)]
    pub async fn list_children(
        &self,
        handle: &ConnectionHandle,
        db_index: u32,
        prefix: &str,
        cursor: u64,
        count: u32,
        sep: Option<&str>,
        no_ttl_only: bool,
        key_type: Option<&str>,
        with_memory: bool,
        budget: Option<u64>,
    ) -> Result<crate::ops::tree::ChildrenPage, DriverError> {
        let t0 = std::time::Instant::now();
        tracing::info!(db_index, %prefix, cursor, count, "redis list_children: acquiring lock");
        let mut conns = self.connections.write().await;
        tracing::info!(
            lock_ms = t0.elapsed().as_millis() as u64,
            "redis list_children: lock acquired"
        );
        let rc = Self::get_conn(&mut conns, handle)?;
        let topology = rc.live.topology();
        Self::select_db(&mut rc.live, db_index)
            .await
            .map_err(DriverError::QueryFailed)?;
        let prefix = prefix.to_string();
        let sep = sep.map(str::to_string);
        let key_type = key_type.map(str::to_string);
        with_redis_conn!(&mut rc.live, |conn| crate::ops::tree::list_children_page(
            conn,
            &prefix,
            cursor,
            count,
            sep.as_deref(),
            no_ttl_only,
            key_type.as_deref(),
            with_memory,
            budget,
            topology,
            t0
        )
        .await)
    }

    pub async fn scan_values(
        &self,
        handle: &ConnectionHandle,
        db_index: u32,
        input: &serde_json::Value,
    ) -> Result<serde_json::Value, DriverError> {
        let mut conns = self.connections.write().await;
        let rc = Self::get_conn(&mut conns, handle)?;
        Self::select_db(&mut rc.live, db_index)
            .await
            .map_err(DriverError::QueryFailed)?;
        let session = handle.pool_id.clone();
        with_redis_conn!(&mut rc.live, |conn| {
            crate::ops::value_search::run_scan_batch(conn, &session, input).await
        })
    }

    pub async fn scan_abort(
        &self,
        handle: &ConnectionHandle,
        input: &serde_json::Value,
    ) -> Result<serde_json::Value, DriverError> {
        let session = handle.pool_id.clone();
        let requested = input
            .get("taskId")
            .or_else(|| input.get("task_id"))
            .and_then(serde_json::Value::as_str);
        Ok(crate::ops::value_search::abort_task(&session, requested).await)
    }

    plugin_on_db!(plugin_set_string, (key: &str, value: &str, keep_ttl: Option<bool>) -> crate::ops::write::SetStringOutcome, |conn| crate::ops::write::set_string_with_ttl_policy(conn, key, value.as_bytes(), keep_ttl));
    plugin_on_db!(plugin_set_string_bytes, (key: &str, bytes: &[u8], keep_ttl: Option<bool>) -> crate::ops::write::SetStringOutcome, |conn| crate::ops::write::set_string_with_ttl_policy(conn, key, bytes, keep_ttl));
    plugin_on_db!(plugin_set_expire_at, (key: &str, expire_at: i64) -> (), |conn| crate::ops::set_expire_at(conn, key, expire_at));
    plugin_on_db!(plugin_hash_set, (key: &str, field: &str, value: &str) -> (), |conn| crate::ops::hash_set(conn, key, field, value));
    plugin_on_db!(plugin_hash_del, (key: &str, fields: &[String]) -> (), |conn| crate::ops::hash_del(conn, key, fields));
    plugin_on_db!(plugin_list_push, (key: &str, side: &str, values: &[String]) -> (), |conn| crate::ops::list_push(conn, key, side, values));
    plugin_on_db!(plugin_list_set, (key: &str, index: i64, value: &str) -> (), |conn| crate::ops::list_set(conn, key, index, value));
    plugin_on_db!(plugin_list_pop, (key: &str, side: &str) -> Option<String>, |conn| crate::ops::list_pop(conn, key, side));
    plugin_on_db!(plugin_list_index, (key: &str, index: i64) -> Option<String>, |conn| crate::ops::list_index(conn, key, index));
    plugin_on_db!(plugin_list_rem, (key: &str, count: i64, value: &str) -> i64, |conn| crate::ops::list_rem(conn, key, count, value));
    plugin_on_db!(plugin_set_add, (key: &str, members: &[String]) -> (), |conn| crate::ops::set_add(conn, key, members));
    plugin_on_db!(plugin_set_remove, (key: &str, members: &[String]) -> (), |conn| crate::ops::set_remove(conn, key, members));
    plugin_on_db!(plugin_zset_add, (key: &str, members: &[crate::ops::ZsetMember]) -> (), |conn| crate::ops::zset_add(conn, key, members));
    plugin_on_db!(plugin_zset_remove, (key: &str, members: &[String]) -> (), |conn| crate::ops::zset_remove(conn, key, members));
    plugin_on_db!(plugin_hash_scan, (key: &str, cursor: u64, count: u32, match_pattern: Option<&str>) -> (u64, Vec<(String, String)>), |conn| crate::ops::hash_scan(conn, key, cursor, count, match_pattern));
    plugin_on_db!(plugin_list_range, (key: &str, start: i64, stop: i64) -> Vec<String>, |conn| crate::ops::list_range(conn, key, start, stop));
    plugin_on_db!(plugin_set_scan, (key: &str, cursor: u64, count: u32, match_pattern: Option<&str>) -> (u64, Vec<String>), |conn| crate::ops::set_scan(conn, key, cursor, count, match_pattern));
    plugin_on_db!(plugin_zset_scan, (key: &str, cursor: u64, count: u32, match_pattern: Option<&str>) -> (u64, Vec<(String, f64)>), |conn| crate::ops::zset_scan(conn, key, cursor, count, match_pattern));
    plugin_on_db!(plugin_delete_keys, (keys: &[String]) -> u64, |conn| crate::ops::delete_keys(conn, keys));
    plugin_on_db!(plugin_rename_key, (key: &str, new_key: &str) -> (), |conn| crate::ops::rename_key(conn, key, new_key));
    plugin_on_db!(plugin_set_ttl, (key: &str, ttl_seconds: i64) -> (), |conn| crate::ops::set_ttl(conn, key, ttl_seconds));
    plugin_on_db!(plugin_batch_delete_pattern, (pattern: &str) -> crate::ops::BatchDeleteResult, |conn| crate::ops::batch_delete_pattern(conn, pattern));
    plugin_on_db!(plugin_batch_set_ttl, (keys: &[String], ttl_seconds: i64) -> crate::ops::BatchSetTtlResult, |conn| crate::ops::batch_set_ttl(conn, keys, ttl_seconds));
    // Key-tree census under one action budget: `truncated` says the number is a
    // floor (the UI renders `n+`), and `*` / an exact key name answer without
    // scanning at all. The topology is passed because the exact-key short circuit
    // sends one addressed `EXISTS` on a cluster.
    plugin_on_db_topo!(plugin_count_matching, (pattern: &str, budget: Option<u64>) -> crate::ops::tree::scan::CountOutcome, |conn, topology| crate::ops::tree::scan::count_budgeted(conn, pattern, budget, topology));
    // Key-attribute probe for one named key: `EXISTS + TYPE + PTTL + MEMORY
    // USAGE` in one batch, never a value read. On a cluster the batch is
    // addressed to the shard owning the key, where it stays in one slot.
    plugin_on_db_topo!(plugin_key_probe, (key: &str) -> crate::ops::key_probe::KeyProbe, |conn, topology| crate::ops::key_probe::key_probe(conn, key, topology));
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

    pub(crate) async fn test_connection_inner(
        &self,
        config: &ConnectionConfig,
    ) -> Result<ServerInfo, DriverError> {
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

    pub async fn plugin_info(
        &self,
        connection_id: &str,
        section: Option<String>,
        node_addr: Option<String>,
    ) -> Result<String, DriverError> {
        if let Some(addr) = node_addr.filter(|s| !s.trim().is_empty()) {
            let plan = self.connection_plan(connection_id).await?;
            let mut conn = open_pinned_node_conn(&plan, addr.trim()).await?;
            return crate::ops::observe::fetch_info(&mut conn, section.as_deref())
                .await
                .map_err(DriverError::QueryFailed);
        }
        with_live_any_op!(self, connection_id, |conn| {
            crate::ops::observe::fetch_info(conn, section.as_deref()).await
        })
    }

    pub async fn plugin_cluster_nodes(
        &self,
        connection_id: &str,
    ) -> Result<crate::ops::cluster::ClusterNodesResult, DriverError> {
        with_live_any_op!(self, connection_id, |conn| {
            crate::ops::cluster::cluster_nodes(conn).await
        })
    }

    pub async fn plugin_memory_sample(
        &self,
        connection_id: &str,
        db_index: u32,
        limit: Option<u32>,
    ) -> Result<crate::ops::observe::MemorySampleResult, DriverError> {
        let limit = crate::ops::observe::resolve_memory_sample_limit(limit);
        // The topology is passed because the batched MEMORY/TYPE/PTTL read is a
        // single pipeline on a single node but has to be addressed key by key on
        // a cluster (a cross-slot pipeline is rejected with `CROSSSLOT`).
        with_live_op_topo!(self, connection_id, db_index, |conn, topology| {
            crate::ops::observe::memory_sample(conn, limit, topology).await
        })
    }

    // Workbench KV context bar: cursor-sampled type distribution. The sample
    // window is clamped inside the op, so an oversized `sampleLimit` never
    // turns into an error; the topology is passed in because a cluster
    // connection cannot carry the cross-key TYPE batch.
    // (Plain comment: rustdoc ignores macro invocations.)
    plugin_on_db_topo!(plugin_type_distribution, (sample_limit: Option<u64>) -> crate::ops::workbench::TypeDistribution, |conn, topology| crate::ops::workbench::type_distribution(conn, sample_limit, topology));

    // Key-attribute sidebar: every attribute in one batch, degraded per field;
    // a missing key is a successful reply with `missing: true`. On a cluster
    // connection the same six commands are sent one at a time, which is what
    // keeps `OBJECT FREQ` from failing the whole probe there.
    plugin_on_db_topo!(plugin_key_object_info, (key: &str) -> crate::ops::workbench::KeyObjectInfo, |conn, topology| crate::ops::workbench::key_object_info(conn, key, topology));

    pub async fn plugin_slowlog_get(
        &self,
        connection_id: &str,
        count: u32,
    ) -> Result<Vec<crate::ops::observe::SlowlogEntry>, DriverError> {
        with_live_any_op!(self, connection_id, |conn| {
            crate::ops::observe::slowlog_get(conn, count).await
        })
    }

    pub async fn plugin_slowlog_reset(&self, connection_id: &str) -> Result<(), DriverError> {
        with_live_any_op!(self, connection_id, |conn| {
            crate::ops::observe::slowlog_reset(conn).await
        })
    }

    pub async fn plugin_modules_list(
        &self,
        connection_id: &str,
    ) -> Result<Vec<String>, DriverError> {
        with_live_any_op!(self, connection_id, |conn| {
            crate::ops::observe::modules_list(conn).await
        })
    }

    pub async fn plugin_memory_usage_key(
        &self,
        connection_id: &str,
        key: &str,
    ) -> Result<crate::ops::observe::MemoryUsageResult, DriverError> {
        with_live_any_op!(self, connection_id, |conn| {
            crate::ops::observe::memory_usage_key(conn, key).await
        })
    }

    pub async fn plugin_info_filtered(
        &self,
        connection_id: &str,
        section: Option<String>,
        search: Option<String>,
        node_addr: Option<String>,
    ) -> Result<crate::ops::observe::InfoFilteredResult, DriverError> {
        if let Some(addr) = node_addr.filter(|s| !s.trim().is_empty()) {
            let plan = self.connection_plan(connection_id).await?;
            let mut conn = open_pinned_node_conn(&plan, addr.trim()).await?;
            return crate::ops::observe::info_filtered(
                &mut conn,
                section.as_deref(),
                search.as_deref(),
            )
            .await
            .map_err(DriverError::QueryFailed);
        }
        with_live_any_op!(self, connection_id, |conn| {
            crate::ops::observe::info_filtered(conn, section.as_deref(), search.as_deref()).await
        })
    }

    pub async fn plugin_exec(
        &self,
        connection_id: &str,
        db_index: u32,
        commands: &str,
        node_addr: Option<String>,
    ) -> Result<crate::ops::exec::ExecResponse, DriverError> {
        let commands = commands.to_string();
        if let Some(addr) = node_addr.filter(|s| !s.trim().is_empty()) {
            let plan = self.connection_plan(connection_id).await?;
            let mut conn = open_pinned_node_conn(&plan, addr.trim()).await?;
            select_db_on(&mut conn, db_index)
                .await
                .map_err(DriverError::QueryFailed)?;
            return crate::ops::exec::exec_commands(&mut conn, &commands)
                .await
                .map_err(DriverError::QueryFailed);
        }
        with_live_op!(self, connection_id, db_index, |conn| {
            crate::ops::exec::exec_commands(conn, &commands).await
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
            crate::ops::pubsub::publish(conn, &channel, &message).await
        })
    }

    plugin_on_db!(plugin_json_get, (key: &str, path: &str, raw: bool) -> crate::ops::json::JsonGetResult, |conn| crate::ops::json::json_get(conn, key, path, raw));
    plugin_on_db!(plugin_json_set, (key: &str, path: &str, value: &str) -> (), |conn| crate::ops::json::json_set(conn, key, path, value));
    plugin_on_db!(plugin_json_del, (key: &str, path: &str) -> crate::ops::json::JsonDelResult, |conn| crate::ops::json::json_del(conn, key, path));

    plugin_on_db!(
        plugin_xrange,
        (key: &str, start: &str, end: &str, count: Option<u32>) -> crate::ops::stream::XrangeResult,
        |conn| crate::ops::stream::xrange(conn, key, start, end, count)
    );

    pub async fn plugin_xadd(
        &self,
        connection_id: &str,
        db_index: u32,
        key: &str,
        fields: &std::collections::HashMap<String, String>,
        id: Option<String>,
    ) -> Result<crate::ops::stream::XaddResult, DriverError> {
        let key = key.to_string();
        let fields = fields.clone();
        with_live_op!(self, connection_id, db_index, |conn| {
            crate::ops::stream::xadd(conn, &key, &fields, id.as_deref()).await
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
            crate::ops::stream::xgroup_create(conn, &key, &group, start_id.as_deref()).await
        })
    }

    plugin_on_db!(
        plugin_xgroup_destroy,
        (key: &str, group: &str) -> (),
        |conn| crate::ops::stream::xgroup_destroy(conn, key, group)
    );

    plugin_on_db!(
        plugin_xinfo_groups,
        (key: &str) -> Vec<crate::ops::stream::StreamGroupInfo>,
        |conn| crate::ops::stream::xinfo_groups(conn, key)
    );

    pub async fn plugin_xinfo_consumers(
        &self,
        connection_id: &str,
        db_index: u32,
        key: &str,
        group: &str,
    ) -> Result<Vec<crate::ops::stream::ConsumerInfo>, DriverError> {
        let key = key.to_string();
        let group = group.to_string();
        with_live_op!(self, connection_id, db_index, |conn| {
            crate::ops::stream::xinfo_consumers(conn, &key, &group).await
        })
    }

    pub async fn plugin_stream_lag(
        &self,
        connection_id: &str,
        db_index: u32,
        key: &str,
        group: &str,
    ) -> Result<crate::ops::stream::StreamLagResult, DriverError> {
        let key = key.to_string();
        let group = group.to_string();
        with_live_op!(self, connection_id, db_index, |conn| {
            crate::ops::stream::stream_lag(conn, &key, &group).await
        })
    }

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
    ) -> Result<crate::ops::stream::XpendingResult, DriverError> {
        let key = key.to_string();
        let group = group.to_string();
        with_live_op!(self, connection_id, db_index, |conn| {
            crate::ops::stream::xpending(
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
            crate::ops::stream::xack(conn, &key, &group, &ids).await
        })
    }

    pub async fn plugin_stream_overview(
        &self,
        connection_id: &str,
        db_index: u32,
        limit: Option<u32>,
    ) -> Result<crate::ops::stream::StreamOverviewResult, DriverError> {
        let limit = crate::ops::stream::resolve_stream_overview_limit(limit);
        with_live_op!(self, connection_id, db_index, |conn| {
            crate::ops::stream::stream_overview(conn, limit).await
        })
    }

    pub async fn plugin_dump_keys(
        &self,
        connection_id: &str,
        db_index: u32,
        keys: &[String],
    ) -> Result<crate::ops::io::DumpKeysResult, DriverError> {
        let keys = keys.to_vec();
        with_live_op!(self, connection_id, db_index, |conn| {
            crate::ops::io::dump_keys(conn, &keys).await
        })
    }

    pub async fn plugin_restore_keys(
        &self,
        connection_id: &str,
        db_index: u32,
        entries: Vec<crate::ops::io::RestoreKeyEntry>,
        replace: bool,
    ) -> Result<crate::ops::io::RestoreKeysResult, DriverError> {
        with_live_op!(self, connection_id, db_index, |conn| {
            crate::ops::io::restore_keys(conn, &entries, replace).await
        })
    }
}

pub(crate) use crate::value::{parse_redis_command_args, parse_scan_result};
