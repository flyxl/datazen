//! Redis driver — exposes Redis as a key-value browser through the DatabaseDriver trait.
//!
//! Unlike SQL databases, Redis doesn't have tables/schemas. We map the concepts:
//! - "databases" → non-empty logical databases (`db0`, `db1`, …)
//! - "tables" → keys in the selected database (as `TableInfo` for tree browsing)
//! - "query" → raw Redis command execution (e.g. `GET key`, `"HGETALL" key` with quotes)
//! - `scan_keys_with_info` / `get_key_detail` — KV browser commands

use super::*;
use super::traits::KeyValueDriver;
use async_trait::async_trait;
use redis::aio::MultiplexedConnection;
use redis::AsyncCommands;
use redis::Client;
use redis::FromRedisValue;
use std::collections::HashMap;
use std::time::Instant;
use tokio::sync::RwLock;

struct RedisConn {
    #[allow(dead_code)]
    client: Client,
    connection: MultiplexedConnection,
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

    /// Redis only accepts a numeric db index in the URL path; non-numeric values default to `0`.
    fn redis_db_index(raw: Option<&str>) -> String {
        let s = raw.map(str::trim).unwrap_or("");
        if s.is_empty() {
            return "0".into();
        }
        if s.chars().all(|c| c.is_ascii_digit()) {
            s.to_string()
        } else {
            "0".into()
        }
    }

    fn build_url(config: &ConnectionConfig) -> String {
        let host = config.host.as_deref().unwrap_or("127.0.0.1");
        let port = config.port.unwrap_or(6379);
        let db = Self::redis_db_index(config.database.as_deref());
        let password = config
            .password
            .as_deref()
            .map(|p| urlencoding::encode(p));
        let username = config
            .username
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|u| urlencoding::encode(u));

        match (username, password) {
            (Some(user), Some(pass)) => {
                format!("redis://{}:{}@{}:{}/{}", user, pass, host, port, db)
            }
            (None, Some(pass)) => format!("redis://:{}@{}:{}/{}", pass, host, port, db),
            (Some(user), None) => format!("redis://{}@{}:{}/{}", user, host, port, db),
            (None, None) => format!("redis://{}:{}/{}", host, port, db),
        }
    }

    /// Parse a logical database name (`db0`, `db7`) or a bare number into a Redis DB index.
    pub fn parse_db_name(database: &str) -> Result<u32, DriverError> {
        let s = database.trim();
        if s.is_empty() {
            return Err(DriverError::QueryFailed("empty database name".into()));
        }
        if let Some(rest) = s.strip_prefix("db") {
            rest
                .parse()
                .map_err(|_| DriverError::QueryFailed("invalid database name (expected e.g. db0)".into()))
        } else {
            s.parse()
                .map_err(|_| DriverError::QueryFailed("invalid database name (expected e.g. db0)".into()))
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
        tracing::info!(lock_ms = t0.elapsed().as_millis() as u64, "redis scan_keys_with_info: lock acquired");
        let rc = Self::get_conn(&mut conns, handle)?;
        let conn = &mut rc.connection;

        let _: () = redis::cmd("SELECT")
            .arg(db_index)
            .query_async(conn)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
        let db_size: u64 = redis::cmd("DBSIZE")
            .query_async(conn)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
        tracing::info!(db_index, db_size, ms = t0.elapsed().as_millis() as u64, "redis scan_keys_with_info: SELECT+DBSIZE done");

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
        tracing::info!(key_count = key_names.len(), next_cursor, ms = t0.elapsed().as_millis() as u64, "redis scan_keys_with_info: SCAN done");

        if key_names.is_empty() {
            return Ok((next_cursor, vec![], db_size));
        }

        // TYPE + TTL for each key
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
            let tk = value_to_type_string(tval);
            let ttl: i64 = FromRedisValue::from_redis_value(ttlval)
                .map_err(|e| DriverError::QueryFailed(format!("TTL: {e}")))?;
            types.push(tk);
            ttls.push(ttl);
        }

        tracing::info!(key_count = key_names.len(), ms = t0.elapsed().as_millis() as u64, "redis scan_keys_with_info: TYPE+TTL done");

        // Size
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
                "none" | _ => {
                    pipe2.cmd("PING");
                }
            }
        }
        let r2: Vec<redis::Value> = pipe2
            .query_async(conn)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

        tracing::info!(key_count = key_names.len(), ms = t0.elapsed().as_millis() as u64, "redis scan_keys_with_info: SIZE done");

        // Preview
        let mut pipe3 = redis::pipe();
        for (k, tk) in key_names.iter().zip(&types) {
            match tk.as_str() {
                "string" => {
                    // Only fetch first 256 bytes instead of full value
                    pipe3.cmd("GETRANGE").arg(k).arg(0i64).arg(255i64);
                }
                "hash" => {
                    // HSCAN with small COUNT instead of HGETALL (which fetches ALL fields)
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
                "none" | _ => {
                    pipe3.cmd("PING");
                }
            }
        }
        let r3: Vec<redis::Value> = pipe3
            .query_async(conn)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

        tracing::info!(key_count = key_names.len(), ms = t0.elapsed().as_millis() as u64, "redis scan_keys_with_info: PREVIEW done");

        let mut keys = Vec::with_capacity(key_names.len());
        for i in 0..key_names.len() {
            let k = &key_names[i];
            let tk = &types[i];
            let ttl = ttls[i];
            let size = if matches!(tk.as_str(), "none") {
                0u64
            } else {
                value_to_u64(r2.get(i).ok_or_else(|| DriverError::QueryFailed("size pipeline".into()))?)
            };
            let preview = if tk == "none" {
                String::new()
            } else {
                preview_value_to_string(r3.get(i).ok_or_else(|| DriverError::QueryFailed("preview pipeline".into()))?, tk)
            };
            keys.push(KeyEntry {
                key: k.clone(),
                key_type: tk.clone(),
                ttl,
                size,
                preview: truncate_preview(&preview, 512),
            });
        }

        Ok((next_cursor, keys, db_size))
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
        tracing::info!(lock_ms = t0.elapsed().as_millis() as u64, "redis get_key_detail: lock acquired");
        let rc = Self::get_conn(&mut conns, handle)?;
        let conn = &mut rc.connection;

        let _: () = redis::cmd("SELECT")
            .arg(db_index)
            .query_async(conn)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
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
                let s = value_to_string(&raw);
                serde_json::json!({ "value": s })
            }
            "hash" => {
                let raw: redis::Value = redis::cmd("HGETALL")
                    .arg(key)
                    .query_async(conn)
                    .await
                    .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
                let obj = redis_flat_pairs_to_map(&raw);
                serde_json::json!({ "fields": serde_json::Value::Object(obj) })
            }
            "list" => {
                let raw: redis::Value = redis::cmd("LRANGE")
                    .arg(key)
                    .arg(0i64)
                    .arg(-1i64)
                    .query_async(conn)
                    .await
                    .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
                let arr = redis_array_to_json_strings(&raw);
                serde_json::json!({ "items": arr })
            }
            "set" => {
                let raw: redis::Value = redis::cmd("SMEMBERS")
                    .arg(key)
                    .query_async(conn)
                    .await
                    .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
                let arr = redis_array_to_json_strings(&raw);
                serde_json::json!({ "members": arr })
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
                let members = redis_zset_to_json(&raw);
                serde_json::json!({ "members": members })
            }
            "stream" => {
                let len: u64 = redis::cmd("XLEN")
                    .arg(key)
                    .query_async(conn)
                    .await
                    .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
                // Pull up to 10k stream entries
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
            key_type: key_type.clone(),
            ttl,
            value,
        })
    }
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
fn parse_scan_result(v: &redis::Value) -> (u64, Vec<String>) {
    match v {
        redis::Value::Array(items) if items.len() >= 2 => {
            let next_cursor: u64 = match &items[0] {
                redis::Value::BulkString(b) => {
                    String::from_utf8_lossy(b).parse().unwrap_or(0)
                }
                redis::Value::Int(i) => *i as u64,
                _ => 0,
            };
            let keys = match &items[1] {
                redis::Value::Array(arr) => arr
                    .iter()
                    .map(|v| value_to_string(v))
                    .collect(),
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
                if i > 0 { s.push_str(", "); }
                s.push_str(&value_field_for_preview(fk));
                s.push_str(": ");
                s.push_str(&value_field_for_preview(fv));
            }
            s.push(')');
            if pairs.len() > 2 { s.push_str(" …"); }
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
                    if i + 2 < n { s.push_str(", "); }
                }
            }
            s.push(')');
            if fields.len() > 4 { s.push_str(" …"); }
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
                    if i + 2 < n { s.push_str(", "); }
                }
            }
            s.push(')');
            if items.len() > 4 { s.push_str(" …"); }
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
fn parse_redis_command_args(s: &str) -> Result<Vec<String>, DriverError> {
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
        DatabaseType::Redis
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
        let url = Self::build_url(config);
        let client = Client::open(url)
            .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;

        let mut conn = client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;

        let info: String = redis::cmd("INFO")
            .arg("server")
            .query_async(&mut conn)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

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
        let url = Self::build_url(config);
        let pool_id = format!("redis_{}", uuid::Uuid::new_v4());

        let client = Client::open(url)
            .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;

        let connection = client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;

        let mut conns = self.connections.write().await;
        conns.insert(pool_id.clone(), RedisConn { client, connection });

        Ok(ConnectionHandle {
            id: pool_id.clone(),
            pool_id,
        })
    }

    async fn disconnect(&self, handle: ConnectionHandle) -> Result<(), DriverError> {
        let t0 = std::time::Instant::now();
        tracing::info!(pool_id = %handle.pool_id, "redis disconnect: acquiring lock");
        let mut conns = self.connections.write().await;
        tracing::info!(lock_ms = t0.elapsed().as_millis() as u64, "redis disconnect: lock acquired");
        conns.remove(&handle.pool_id);
        tracing::info!("redis disconnect: done");
        Ok(())
    }

    async fn get_databases(&self, handle: &ConnectionHandle) -> Result<Vec<String>, DriverError> {
        let t0 = std::time::Instant::now();
        tracing::info!("redis get_databases: acquiring lock");
        let mut conns = self.connections.write().await;
        tracing::info!(lock_ms = t0.elapsed().as_millis() as u64, "redis get_databases: lock acquired");
        let rc = Self::get_conn(&mut conns, handle)?;

        // Single-command approach: INFO keyspace returns all non-empty dbs at once
        // e.g. "# Keyspace\r\ndb0:keys=732,expires=0\r\ndb1:keys=3886,expires=100\r\n"
        let info_raw: redis::Value = redis::cmd("INFO")
            .arg("keyspace")
            .query_async(&mut rc.connection)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

        let info_str = value_to_string(&info_raw);
        let mut out = Vec::new();
        for line in info_str.lines() {
            let line = line.trim();
            // lines look like "db0:keys=732,expires=0,avg_ttl=0"
            if let Some(colon) = line.find(':') {
                let db_name = &line[..colon];
                if db_name.starts_with("db") && db_name[2..].chars().all(|c| c.is_ascii_digit()) {
                    out.push(db_name.to_string());
                }
            }
        }
        out.sort_by_key(|s| s[2..].parse::<u32>().unwrap_or(0));
        tracing::info!(count = out.len(), ms = t0.elapsed().as_millis() as u64, "redis get_databases: done");
        Ok(out)
    }

    async fn get_tables(
        &self,
        handle: &ConnectionHandle,
        database: &str,
    ) -> Result<Vec<TableInfo>, DriverError> {
        let t0 = std::time::Instant::now();
        let db_index = Self::parse_db_name(database)?;
        tracing::info!(%database, db_index, "redis get_tables: acquiring lock");
        let mut conns = self.connections.write().await;
        tracing::info!(lock_ms = t0.elapsed().as_millis() as u64, "redis get_tables: lock acquired");
        let rc = Self::get_conn(&mut conns, handle)?;
        let conn = &mut rc.connection;

        let _: () = redis::cmd("SELECT")
            .arg(db_index)
            .query_async(conn)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

        const MAX_KEYS: usize = 10_000;
        let mut keys: Vec<String> = Vec::new();
        let mut cursor: u64 = 0;
        loop {
            let raw: redis::Value = redis::cmd("SCAN")
                .arg(cursor)
                .arg("COUNT")
                .arg(200)
                .query_async(conn)
                .await
                .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

            let (next_cursor, batch) = parse_scan_result(&raw);
            keys.extend(batch);
            cursor = next_cursor;
            if cursor == 0 || keys.len() >= MAX_KEYS {
                break;
            }
        }
        tracing::info!(%database, key_count = keys.len(), ms = t0.elapsed().as_millis() as u64, "redis get_tables: scan done");

        keys.sort();

        let tables: Vec<TableInfo> = keys
            .into_iter()
            .map(|key| TableInfo {
                name: key,
                schema: None,
                table_type: TableType::Table,
                row_count: None,
            })
            .collect();

        Ok(tables)
    }

    async fn get_table_schema(
        &self,
        handle: &ConnectionHandle,
        table: &str,
    ) -> Result<TableSchema, DriverError> {
        let mut conns = self.connections.write().await;
        let rc = Self::get_conn(&mut conns, handle)?;

        let key_type: String = rc
            .connection
            .key_type(table)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

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
            table_name: table.to_string(),
            columns,
            primary_keys: vec![],
            indexes: vec![],
            foreign_keys: vec![],
        })
    }

    async fn query(&self, handle: &ConnectionHandle, sql: &str) -> Result<QueryResult, DriverError> {
        let start = Instant::now();
        let parts = parse_redis_command_args(sql)?;

        tracing::info!(cmd = %sql, "redis query: acquiring lock");
        let mut conns = self.connections.write().await;
        tracing::info!(lock_ms = start.elapsed().as_millis() as u64, "redis query: lock acquired");
        let rc = Self::get_conn(&mut conns, handle)?;

        let mut cmd = redis::cmd(parts[0].as_str());
        for part in &parts[1..] {
            cmd.arg(part as &str);
        }

        let result: redis::Value = cmd
            .query_async(&mut rc.connection)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

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
        let commands: Vec<&str> = sql.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
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

    async fn cancel_query(&self, _handle: &ConnectionHandle) -> Result<(), DriverError> {
        Ok(())
    }
}

#[async_trait]
impl KeyValueDriver for RedisDriver {
    fn driver_type(&self) -> DatabaseType {
        DatabaseType::Redis
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
            vec![ColumnInfo { name: "result".into(), data_type: "string".into(), nullable: true }],
            vec![vec![Some(Value::Null)]],
        ),
        redis::Value::Int(n) => (
            vec![ColumnInfo { name: "result".into(), data_type: "integer".into(), nullable: false }],
            vec![vec![Some(Value::Integer(*n))]],
        ),
        redis::Value::BulkString(bytes) => {
            let s = String::from_utf8_lossy(bytes).to_string();
            (
                vec![ColumnInfo { name: "result".into(), data_type: "string".into(), nullable: false }],
                vec![vec![Some(Value::String(s))]],
            )
        }
        redis::Value::VerbatimString { text, .. } => {
            (
                vec![ColumnInfo { name: "result".into(), data_type: "string".into(), nullable: false }],
                vec![vec![Some(Value::String(text.clone()))]],
            )
        }
        redis::Value::Array(items) => {
            if items.len() >= 2 && items.len() % 2 == 0 && looks_like_hash(items) {
                let columns = vec![
                    ColumnInfo { name: "field".into(), data_type: "string".into(), nullable: false },
                    ColumnInfo { name: "value".into(), data_type: "string".into(), nullable: true },
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
                    ColumnInfo { name: "index".into(), data_type: "integer".into(), nullable: false },
                    ColumnInfo { name: "value".into(), data_type: "string".into(), nullable: true },
                ];
                let rows: Vec<Vec<Option<Value>>> = items
                    .iter()
                    .enumerate()
                    .map(|(i, v)| {
                        vec![Some(Value::Integer(i as i64)), Some(redis_to_value(v))]
                    })
                    .collect();
                (columns, rows)
            }
        }
        redis::Value::SimpleString(s) => (
            vec![ColumnInfo { name: "result".into(), data_type: "string".into(), nullable: false }],
            vec![vec![Some(Value::String(s.clone()))]],
        ),
        #[allow(deprecated)]
        redis::Value::Okay => (
            vec![ColumnInfo { name: "result".into(), data_type: "string".into(), nullable: false }],
            vec![vec![Some(Value::String("OK".into()))]],
        ),
        _ => (
            vec![ColumnInfo { name: "result".into(), data_type: "string".into(), nullable: false }],
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
    items
        .chunks(2)
        .all(|pair| {
            matches!(
                &pair[0],
                redis::Value::BulkString(_) | redis::Value::SimpleString(_)
            )
        })
}
