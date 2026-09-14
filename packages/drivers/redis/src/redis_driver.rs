//! Redis driver core — see redis_driver_on / redis_driver_traits / redis_value for the rest.

use datazen_driver_api::*;
use redis::AsyncCommands;
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::RwLock;

use crate::connect::{
    build_connection_plan, looks_like_connection_loss, open_live_conn, open_pinned_node_conn,
    ConnectionPlan, RedisLiveConn,
};
use crate::redis_driver_on::{
    get_key_detail_on, info_server_on, scan_keys_with_info_on, select_db_on,
};
use crate::with_redis_conn;

pub(crate) struct RedisConn {
    pub(crate) plan: ConnectionPlan,
    pub(crate) live: RedisLiveConn,
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

    pub async fn scan_keys_with_info(
        &self,
        handle: &ConnectionHandle,
        db_index: u32,
        pattern: &str,
        cursor: u64,
        count: u32,
        key_type: Option<&str>,
        with_memory: bool,
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
        let key_type = key_type.map(str::to_string);
        with_redis_conn!(&mut rc.live, |conn| scan_keys_with_info_on(
            conn,
            db_index,
            &pattern,
            cursor,
            count,
            key_type.as_deref(),
            with_memory,
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

    plugin_on_db!(plugin_set_string, (key: &str, value: &str, keep_ttl: bool) -> (), |conn| crate::ops::set_string_with_options(conn, key, value, keep_ttl));
    plugin_on_db!(plugin_set_expire_at, (key: &str, expire_at: i64) -> (), |conn| crate::ops::set_expire_at(conn, key, expire_at));
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
        with_live_any_op!(self, connection_id, |conn| crate::ops::flush_all(conn).await)
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

pub(crate) use crate::redis_value::{parse_scan_result, parse_redis_command_args};
