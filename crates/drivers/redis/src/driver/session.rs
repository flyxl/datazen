//! Connection-scoped Redis operations (SELECT, SCAN, GET detail, tables).

use base64::Engine;
use datazen_driver_api::*;
use redis::AsyncCommands;

use crate::types::{KeyDetail, ValueFrame};

pub(crate) async fn select_db_on<C>(conn: &mut C, db_index: u32) -> Result<(), String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    redis::cmd("SELECT")
        .arg(db_index)
        .query_async(conn)
        .await
        .map_err(|e| e.to_string())
}

pub(crate) async fn info_server_on<C>(conn: &mut C) -> Result<String, String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    redis::cmd("INFO")
        .arg("server")
        .query_async(conn)
        .await
        .map_err(|e| e.to_string())
}

pub(crate) async fn query_cmd_on<C>(
    conn: &mut C,
    command: &str,
    args: &[String],
) -> Result<redis::Value, String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    let mut cmd = redis::cmd(command);
    for a in args {
        cmd.arg(a);
    }
    cmd.query_async(conn).await.map_err(|e| e.to_string())
}

pub(crate) async fn get_tables_on<C>(
    conn: &mut C,
    db_index: u32,
    max_keys: usize,
) -> Result<Vec<TableInfo>, DriverError>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    let keys = crate::ops::scan_keys(conn, None, Some(max_keys))
        .await
        .map_err(DriverError::QueryFailed)?;
    let mut tables = Vec::with_capacity(keys.len());
    for key in keys {
        tables.push(TableInfo {
            name: key,
            schema: Some(format!("db{db_index}")),
            table_type: TableType::Table,
            row_count: None,
        });
    }
    Ok(tables)
}

// `scan_keys_with_info_on` moved to `crate::ops::tree::scan::scan_keys_page`,
// which reads DBSIZE once per call instead of once per page and resolves a
// page's attributes in two batches instead of one round trip per key per
// attribute. What stays here are the single-key reads the value frame needs.

/// Normalize UI/command type filter to a Redis TYPE token, or None for no filter.
pub(crate) fn normalize_type_filter(key_type: Option<&str>) -> Option<&'static str> {
    let raw = key_type?.trim();
    if raw.is_empty() || raw == "*" || raw.eq_ignore_ascii_case("all") {
        return None;
    }
    let lower = raw.to_ascii_lowercase();
    match lower.as_str() {
        "string" => Some("string"),
        "list" => Some("list"),
        "set" => Some("set"),
        "zset" | "sortedset" | "sorted_set" => Some("zset"),
        "hash" => Some("hash"),
        "stream" => Some("stream"),
        "rejson" | "json" | "rejson-rl" => Some("ReJSON-RL"),
        _ => None,
    }
}

async fn type_of_key_on<C>(conn: &mut C, key: &str) -> Result<String, String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    let ty: String = redis::cmd("TYPE")
        .arg(key)
        .query_async(conn)
        .await
        .map_err(|e| e.to_string())?;
    Ok(ty)
}

async fn memory_usage_on<C>(conn: &mut C, key: &str) -> Result<usize, String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    let n: Option<i64> = redis::cmd("MEMORY")
        .arg("USAGE")
        .arg(key)
        .query_async(conn)
        .await
        .map_err(|e| e.to_string())?;
    Ok(n.unwrap_or(0).max(0) as usize)
}

async fn value_len_on<C>(conn: &mut C, key: &str, ty: &str) -> Result<usize, String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    let n: i64 = match ty {
        "string" => redis::cmd("STRLEN").arg(key).query_async(conn).await,
        "list" => redis::cmd("LLEN").arg(key).query_async(conn).await,
        "set" => redis::cmd("SCARD").arg(key).query_async(conn).await,
        "zset" => redis::cmd("ZCARD").arg(key).query_async(conn).await,
        "hash" => redis::cmd("HLEN").arg(key).query_async(conn).await,
        "stream" => redis::cmd("XLEN").arg(key).query_async(conn).await,
        _ => Ok(0),
    }
    .map_err(|e| e.to_string())?;
    Ok(n.max(0) as usize)
}

pub(crate) async fn get_key_detail_on<C>(conn: &mut C, key: &str) -> Result<KeyDetail, DriverError>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    let ty = type_of_key_on(conn, key)
        .await
        .map_err(DriverError::QueryFailed)?;
    if ty == "none" {
        return Err(DriverError::QueryFailed(format!("key not found: {key}")));
    }
    let ttl: i64 = redis::cmd("TTL")
        .arg(key)
        .query_async(conn)
        .await
        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
    let value = key_value_json(conn, key, &ty)
        .await
        .map_err(DriverError::QueryFailed)?;
    Ok(KeyDetail {
        key: key.to_string(),
        key_type: ty,
        ttl,
        value,
    })
}

/// Maximum raw value size (5 MiB) before truncation.
const RAW_VALUE_MAX_BYTES: usize = 5 * 1024 * 1024;

/// Binary-safe key value fetch: returns TYPE/TTL/logical length/MEMORY USAGE
/// and the raw bytes of a string key as base64.
pub(crate) async fn get_key_raw_on<C>(
    conn: &mut C,
    key: &str,
    with_memory: bool,
) -> Result<ValueFrame, DriverError>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    let ty = type_of_key_on(conn, key)
        .await
        .map_err(DriverError::QueryFailed)?;
    if ty == "none" {
        return Err(DriverError::QueryFailed(format!("key not found: {key}")));
    }
    let ttl: i64 = redis::cmd("TTL")
        .arg(key)
        .query_async(conn)
        .await
        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
    let logical_len = value_len_on(conn, key, &ty)
        .await
        .map_err(|e| DriverError::QueryFailed(e))? as u64;
    let mem_bytes = if with_memory {
        memory_usage_on(conn, key).await.ok().map(|n| n as u64)
    } else {
        None
    };

    // Raw bytes only for string keys — aggregate types keep raw_b64=None.
    let (raw_b64, truncated) = if ty == "string" {
        let raw: redis::Value = redis::cmd("GET")
            .arg(key)
            .query_async(conn)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
        match raw {
            redis::Value::BulkString(bytes) => {
                if bytes.len() > RAW_VALUE_MAX_BYTES {
                    (None, true)
                } else {
                    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                    (Some(b64), false)
                }
            }
            redis::Value::Nil => (None, false),
            _ => (None, false),
        }
    } else {
        (None, false)
    };

    Ok(ValueFrame {
        key: key.to_string(),
        key_type: ty,
        ttl,
        logical_len,
        mem_bytes,
        raw_b64,
        truncated,
    })
}

async fn key_value_json<C>(conn: &mut C, key: &str, ty: &str) -> Result<serde_json::Value, String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    match ty {
        "string" => {
            let v: Option<String> = redis::cmd("GET")
                .arg(key)
                .query_async(conn)
                .await
                .map_err(|e| e.to_string())?;
            Ok(serde_json::json!({ "value": v.unwrap_or_default() }))
        }
        "hash" => {
            let pairs: Vec<String> = redis::cmd("HGETALL")
                .arg(key)
                .query_async(conn)
                .await
                .map_err(|e| e.to_string())?;
            let mut obj = serde_json::Map::new();
            for chunk in pairs.chunks(2) {
                if chunk.len() == 2 {
                    obj.insert(
                        chunk[0].clone(),
                        serde_json::Value::String(chunk[1].clone()),
                    );
                }
            }
            Ok(serde_json::Value::Object(obj))
        }
        "list" => {
            let vals: Vec<String> = redis::cmd("LRANGE")
                .arg(key)
                .arg(0)
                .arg(-1)
                .query_async(conn)
                .await
                .map_err(|e| e.to_string())?;
            Ok(serde_json::json!(vals))
        }
        "set" => {
            let vals: Vec<String> = redis::cmd("SMEMBERS")
                .arg(key)
                .query_async(conn)
                .await
                .map_err(|e| e.to_string())?;
            Ok(serde_json::json!(vals))
        }
        "zset" => {
            let vals: Vec<String> = redis::cmd("ZRANGE")
                .arg(key)
                .arg(0)
                .arg(-1)
                .arg("WITHSCORES")
                .query_async(conn)
                .await
                .map_err(|e| e.to_string())?;
            let mut members = Vec::new();
            for chunk in vals.chunks(2) {
                if chunk.len() == 2 {
                    let score: f64 = chunk[1].parse().unwrap_or(0.0);
                    members.push(serde_json::json!({ "member": chunk[0], "score": score }));
                }
            }
            Ok(serde_json::json!(members))
        }
        "stream" => {
            let raw: redis::Value = redis::cmd("XRANGE")
                .arg(key)
                .arg("-")
                .arg("+")
                .arg("COUNT")
                .arg(100)
                .query_async(conn)
                .await
                .map_err(|e| e.to_string())?;
            match crate::value::stream_entry_to_json(&raw) {
                Ok(v) => Ok(v),
                Err(()) => Ok(serde_json::json!([])),
            }
        }
        other => Ok(serde_json::json!({ "type": other })),
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_type_filter;

    #[test]
    fn type_filter_none_for_empty_or_all() {
        assert_eq!(normalize_type_filter(None), None);
        assert_eq!(normalize_type_filter(Some("")), None);
        assert_eq!(normalize_type_filter(Some("*")), None);
        assert_eq!(normalize_type_filter(Some("all")), None);
        assert_eq!(normalize_type_filter(Some("ALL")), None);
    }

    #[test]
    fn type_filter_maps_common_aliases() {
        assert_eq!(normalize_type_filter(Some("string")), Some("string"));
        assert_eq!(normalize_type_filter(Some("HASH")), Some("hash"));
        assert_eq!(normalize_type_filter(Some("zset")), Some("zset"));
        assert_eq!(normalize_type_filter(Some("sortedSet")), Some("zset"));
        assert_eq!(normalize_type_filter(Some("json")), Some("ReJSON-RL"));
    }

    #[test]
    fn type_filter_rejects_unknown() {
        assert_eq!(normalize_type_filter(Some("foo")), None);
    }
}
