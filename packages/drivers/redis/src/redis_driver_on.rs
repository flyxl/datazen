//! Connection-scoped Redis operations (SELECT, SCAN, GET detail, tables).

use base64::Engine;
use datazen_driver_api::*;
use redis::AsyncCommands;
use std::time::Instant;

use crate::types::{KeyDetail, ValueFrame};

use crate::redis_value::{preview_value_to_string, truncate_preview};

const PREVIEW_MAX: usize = 120;

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

/// One SCAN page with per-key TYPE / TTL / size / preview.
///
/// - `key_type`: optional Redis type filter (`string`, `hash`, …) via `SCAN … TYPE`
///   (Redis ≥ 6.0). Empty / "*" / "all" means no filter.
/// - `with_memory`: when true, prefer `MEMORY USAGE` for the size column (bytes);
///   otherwise use logical length (STRLEN / LLEN / …).
pub(crate) async fn scan_keys_with_info_on<C>(
    conn: &mut C,
    _db_index: u32,
    pattern: &str,
    cursor: u64,
    count: u32,
    key_type: Option<&str>,
    with_memory: bool,
    no_ttl_only: bool,
    t0: Instant,
) -> Result<(u64, Vec<KeyEntry>, u64), DriverError>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    let match_pat = if !pattern.is_empty() && pattern != "*" {
        Some(pattern)
    } else {
        None
    };
    let (next, keys) = crate::ops::scan_batch(
        conn,
        cursor,
        count,
        match_pat,
        normalize_type_filter(key_type),
    )
    .await
    .map_err(DriverError::QueryFailed)?;
    let mut entries = Vec::with_capacity(keys.len());
    for key in &keys {
        let ty = type_of_key_on(conn, key)
            .await
            .unwrap_or_else(|_| "none".into());
        let ttl: i64 = redis::cmd("TTL")
            .arg(key)
            .query_async(conn)
            .await
            .unwrap_or(-2);
        if no_ttl_only && ttl != -1 {
            continue;
        }
        let size = if with_memory {
            match memory_usage_on(conn, key).await {
                Ok(n) => n,
                Err(_) => value_len_on(conn, key, &ty).await.unwrap_or(0),
            }
        } else {
            value_len_on(conn, key, &ty).await.unwrap_or(0)
        };
        let preview = preview_on(conn, key, &ty).await.unwrap_or_default();
        entries.push(KeyEntry {
            key: key.clone(),
            key_type: ty,
            ttl,
            size: size as u64,
            preview,
        });
    }
    let db_size: i64 = redis::cmd("DBSIZE").query_async(conn).await.unwrap_or(0);
    tracing::info!(
        elapsed_ms = t0.elapsed().as_millis() as u64,
        keys = entries.len(),
        with_memory,
        "redis scan_keys_with_info_on done"
    );
    Ok((next, entries, db_size.max(0) as u64))
}

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

async fn preview_on<C>(conn: &mut C, key: &str, ty: &str) -> Result<String, String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    match ty {
        "string" => {
            let v: redis::Value = redis::cmd("GET")
                .arg(key)
                .query_async(conn)
                .await
                .map_err(|e| e.to_string())?;
            Ok(truncate_preview(
                &preview_value_to_string(&v, "string"),
                PREVIEW_MAX,
            ))
        }
        "list" => {
            let vals: Vec<String> = redis::cmd("LRANGE")
                .arg(key)
                .arg(0)
                .arg(2)
                .query_async(conn)
                .await
                .map_err(|e| e.to_string())?;
            Ok(truncate_preview(&format!("{vals:?}"), PREVIEW_MAX))
        }
        "hash" => {
            // Use HSCAN COUNT 3 instead of HGETALL to avoid loading large hashes.
            let raw: redis::Value = redis::cmd("HSCAN")
                .arg(key)
                .arg(0)
                .arg("COUNT")
                .arg(3)
                .query_async(conn)
                .await
                .map_err(|e| e.to_string())?;
            // HSCAN returns [cursor, [field, value, ...]]
            let vals = extract_hscan_preview(&raw);
            Ok(truncate_preview(&format!("{vals:?}"), PREVIEW_MAX))
        }
        "set" => {
            let vals: Vec<String> = redis::cmd("SRANDMEMBER")
                .arg(key)
                .arg(3)
                .query_async(conn)
                .await
                .unwrap_or_default();
            Ok(truncate_preview(&format!("{vals:?}"), PREVIEW_MAX))
        }
        "zset" => {
            let vals: Vec<String> = redis::cmd("ZRANGE")
                .arg(key)
                .arg(0)
                .arg(2)
                .arg("WITHSCORES")
                .query_async(conn)
                .await
                .unwrap_or_default();
            Ok(truncate_preview(&format!("{vals:?}"), PREVIEW_MAX))
        }
        "stream" => Ok("(stream)".into()),
        _ => Ok(String::new()),
    }
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
            match crate::redis_value::stream_entry_to_json(&raw) {
                Ok(v) => Ok(v),
                Err(()) => Ok(serde_json::json!([])),
            }
        }
        other => Ok(serde_json::json!({ "type": other })),
    }
}

/// Extract field-value pairs from an HSCAN result for preview.
/// HSCAN returns a two-element array: [cursor, [field, value, field, value, ...]]
fn extract_hscan_preview(raw: &redis::Value) -> Vec<String> {
    if let redis::Value::Array(items) = raw {
        if items.len() >= 2 {
            if let redis::Value::Array(members) = &items[1] {
                let mut result = Vec::new();
                for chunk in members.chunks(2) {
                    if chunk.len() == 2 {
                        let field = redis_value_to_string(&chunk[0]);
                        let value = redis_value_to_string(&chunk[1]);
                        result.push(format!("{field}: {value}"));
                    }
                }
                return result;
            }
        }
    }
    Vec::new()
}

fn redis_value_to_string(v: &redis::Value) -> String {
    match v {
        redis::Value::BulkString(bytes) => String::from_utf8_lossy(bytes).into_owned(),
        redis::Value::SimpleString(s) => s.clone(),
        redis::Value::Int(n) => n.to_string(),
        redis::Value::Okay => "OK".into(),
        _ => format!("{v:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::{extract_hscan_preview, normalize_type_filter, redis_value_to_string};

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

    #[test]
    fn redis_value_to_string_bulk_string() {
        let v = redis::Value::BulkString(b"hello".to_vec());
        assert_eq!(redis_value_to_string(&v), "hello");
    }

    #[test]
    fn redis_value_to_string_int() {
        let v = redis::Value::Int(42);
        assert_eq!(redis_value_to_string(&v), "42");
    }

    #[test]
    fn redis_value_to_string_okay() {
        let v = redis::Value::Okay;
        assert_eq!(redis_value_to_string(&v), "OK");
    }

    #[test]
    fn extract_hscan_preview_basic() {
        let raw = redis::Value::Array(vec![
            redis::Value::BulkString(b"0".to_vec()),
            redis::Value::Array(vec![
                redis::Value::BulkString(b"f1".to_vec()),
                redis::Value::BulkString(b"v1".to_vec()),
                redis::Value::BulkString(b"f2".to_vec()),
                redis::Value::BulkString(b"v2".to_vec()),
            ]),
        ]);
        let result = extract_hscan_preview(&raw);
        assert_eq!(result, vec!["f1: v1", "f2: v2"]);
    }

    #[test]
    fn extract_hscan_preview_empty() {
        let raw = redis::Value::Array(vec![
            redis::Value::BulkString(b"0".to_vec()),
            redis::Value::Array(vec![]),
        ]);
        let result = extract_hscan_preview(&raw);
        assert!(result.is_empty());
    }

    #[test]
    fn extract_hscan_preview_malformed() {
        let raw = redis::Value::Int(0);
        let result = extract_hscan_preview(&raw);
        assert!(result.is_empty());
    }
}
