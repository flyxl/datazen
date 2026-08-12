//! Redis Stream helpers: XRANGE/XADD, consumer groups, pending, and overview sampling.

use std::collections::HashMap;

use redis::AsyncCommands;
use serde::Serialize;

use crate::redis_driver::parse_scan_result;

/// Default number of stream keys to sample when `limit` is omitted or zero.
pub const DEFAULT_STREAM_OVERVIEW_LIMIT: u32 = 100;

/// Resolve the effective stream overview limit (defaults to [`DEFAULT_STREAM_OVERVIEW_LIMIT`]).
pub fn resolve_stream_overview_limit(limit: Option<u32>) -> u32 {
    match limit {
        Some(0) | None => DEFAULT_STREAM_OVERVIEW_LIMIT,
        Some(n) => n,
    }
}

/// Validate a consumer group name (non-empty after trim).
pub fn validate_xgroup_name(name: &str) -> Result<String, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("consumer group name is required".into());
    }
    Ok(trimmed.to_string())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamEntry {
    pub id: String,
    pub fields: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct XrangeResult {
    pub entries: Vec<StreamEntry>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct XaddResult {
    pub id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamGroupInfo {
    pub name: String,
    pub consumers: u64,
    pub pending: u64,
    pub last_delivered_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct XpendingEntry {
    pub id: String,
    pub consumer: String,
    pub idle_ms: u64,
    pub delivery_count: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct XpendingResult {
    pub total: u64,
    pub entries: Vec<XpendingEntry>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamOverviewRow {
    pub key: String,
    pub length: u64,
    pub group_count: u64,
    pub pending_total: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamOverviewResult {
    pub rows: Vec<StreamOverviewRow>,
    pub truncated: bool,
}

fn value_to_string(v: &redis::Value) -> String {
    match v {
        redis::Value::BulkString(b) => String::from_utf8_lossy(b).into(),
        redis::Value::SimpleString(s) => s.clone(),
        redis::Value::VerbatimString { text, .. } => text.clone(),
        redis::Value::Int(i) => i.to_string(),
        redis::Value::Okay => "OK".into(),
        redis::Value::Nil => String::new(),
        other => format!("{other:?}"),
    }
}

fn value_to_u64(v: &redis::Value) -> u64 {
    match v {
        redis::Value::Int(i) => *i as u64,
        redis::Value::BulkString(b) => String::from_utf8_lossy(b).parse().unwrap_or(0),
        _ => 0,
    }
}

fn parse_stream_entry(v: &redis::Value) -> Option<StreamEntry> {
    let parts = match v {
        redis::Value::Array(items) if !items.is_empty() => items,
        _ => return None,
    };
    let id = value_to_string(&parts[0]);
    let mut fields = HashMap::new();
    if parts.len() >= 2 {
        if let redis::Value::Array(pairs) = &parts[1] {
            for chunk in pairs.chunks(2) {
                if chunk.len() == 2 {
                    fields.insert(value_to_string(&chunk[0]), value_to_string(&chunk[1]));
                }
            }
        }
    }
    Some(StreamEntry { id, fields })
}

fn parse_xinfo_groups(raw: &redis::Value) -> Result<Vec<StreamGroupInfo>, String> {
    let groups = match raw {
        redis::Value::Array(items) => items,
        _ => return Ok(vec![]),
    };

    let mut out = Vec::new();
    for group in groups {
        let mut name = String::new();
        let mut consumers = 0u64;
        let mut pending = 0u64;
        let mut last_delivered_id = String::new();

        match group {
            redis::Value::Array(fields) => {
                for chunk in fields.chunks(2) {
                    if chunk.len() != 2 {
                        continue;
                    }
                    let key = value_to_string(&chunk[0]).to_ascii_lowercase();
                    let val = &chunk[1];
                    match key.as_str() {
                        "name" => name = value_to_string(val),
                        "consumers" => consumers = value_to_u64(val),
                        "pending" => pending = value_to_u64(val),
                        "last-delivered-id" => last_delivered_id = value_to_string(val),
                        _ => {}
                    }
                }
            }
            redis::Value::Map(pairs) => {
                for (k, v) in pairs {
                    let key = value_to_string(k).to_ascii_lowercase();
                    match key.as_str() {
                        "name" => name = value_to_string(v),
                        "consumers" => consumers = value_to_u64(v),
                        "pending" => pending = value_to_u64(v),
                        "last-delivered-id" => last_delivered_id = value_to_string(v),
                        _ => {}
                    }
                }
            }
            _ => continue,
        }

        if !name.is_empty() {
            out.push(StreamGroupInfo {
                name,
                consumers,
                pending,
                last_delivered_id,
            });
        }
    }

    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

fn parse_xpending_entries(raw: &redis::Value) -> Result<XpendingResult, String> {
    let items = match raw {
        redis::Value::Array(items) => items,
        _ => {
            return Ok(XpendingResult {
                total: 0,
                entries: vec![],
            })
        }
    };

    if items.is_empty() {
        return Ok(XpendingResult {
            total: 0,
            entries: vec![],
        });
    }

    if items.len() >= 4 && matches!(&items[0], redis::Value::Int(_)) {
        let total = value_to_u64(&items[0]);
        return Ok(XpendingResult {
            total,
            entries: vec![],
        });
    }

    let mut entries = Vec::new();
    for item in items {
        let parts = match item {
            redis::Value::Array(parts) if parts.len() >= 4 => parts,
            _ => continue,
        };
        entries.push(XpendingEntry {
            id: value_to_string(&parts[0]),
            consumer: value_to_string(&parts[1]),
            idle_ms: value_to_u64(&parts[2]),
            delivery_count: value_to_u64(&parts[3]),
        });
    }

    Ok(XpendingResult {
        total: entries.len() as u64,
        entries,
    })
}

pub async fn xrange<C>(
    conn: &mut C,
    key: &str,
    start: &str,
    end: &str,
    count: Option<u32>,
) -> Result<XrangeResult, String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    let key = key.trim();
    if key.is_empty() {
        return Err("key is required".into());
    }
    let start = if start.trim().is_empty() {
        "-"
    } else {
        start.trim()
    };
    let end = if end.trim().is_empty() {
        "+"
    } else {
        end.trim()
    };

    let mut cmd = redis::cmd("XRANGE");
    cmd.arg(key).arg(start).arg(end);
    if let Some(n) = count.filter(|&c| c > 0) {
        cmd.arg("COUNT").arg(n);
    }

    let raw: redis::Value = cmd.query_async(conn).await.map_err(|e| e.to_string())?;
    let entries = match raw {
        redis::Value::Array(items) => items.iter().filter_map(parse_stream_entry).collect(),
        _ => vec![],
    };

    Ok(XrangeResult { entries })
}

pub async fn xadd<C>(
    conn: &mut C,
    key: &str,
    fields: &HashMap<String, String>,
    id: Option<&str>,
) -> Result<XaddResult, String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    let key = key.trim();
    if key.is_empty() {
        return Err("key is required".into());
    }
    if fields.is_empty() {
        return Err("at least one field is required".into());
    }

    let mut cmd = redis::cmd("XADD");
    cmd.arg(key);
    cmd.arg(id.filter(|s| !s.trim().is_empty()).unwrap_or("*"));
    for (field, value) in fields {
        let field = field.trim();
        if field.is_empty() {
            continue;
        }
        cmd.arg(field).arg(value);
    }

    let raw: redis::Value = cmd.query_async(conn).await.map_err(|e| e.to_string())?;
    Ok(XaddResult {
        id: value_to_string(&raw),
    })
}

pub async fn xgroup_create<C>(
    conn: &mut C,
    key: &str,
    group: &str,
    start_id: Option<&str>,
) -> Result<(), String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    let key = key.trim();
    if key.is_empty() {
        return Err("key is required".into());
    }
    let group = validate_xgroup_name(group)?;

    let mut cmd = redis::cmd("XGROUP");
    cmd.arg("CREATE").arg(key).arg(&group);
    cmd.arg(start_id.filter(|s| !s.trim().is_empty()).unwrap_or("$"));

    cmd.query_async::<()>(conn).await.map_err(|e| e.to_string())
}

pub async fn xgroup_destroy<C>(conn: &mut C, key: &str, group: &str) -> Result<(), String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    let key = key.trim();
    if key.is_empty() {
        return Err("key is required".into());
    }
    let group = validate_xgroup_name(group)?;

    redis::cmd("XGROUP")
        .arg("DESTROY")
        .arg(key)
        .arg(group)
        .query_async::<()>(conn)
        .await
        .map_err(|e| e.to_string())
}

pub async fn xinfo_groups<C>(conn: &mut C, key: &str) -> Result<Vec<StreamGroupInfo>, String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    let key = key.trim();
    if key.is_empty() {
        return Err("key is required".into());
    }

    let raw: redis::Value = redis::cmd("XINFO")
        .arg("GROUPS")
        .arg(key)
        .query_async(conn)
        .await
        .map_err(|e| e.to_string())?;

    parse_xinfo_groups(&raw)
}

pub async fn xpending<C>(
    conn: &mut C,
    key: &str,
    group: &str,
    start: Option<&str>,
    end: Option<&str>,
    count: Option<u32>,
    consumer: Option<&str>,
) -> Result<XpendingResult, String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    let key = key.trim();
    if key.is_empty() {
        return Err("key is required".into());
    }
    let group = validate_xgroup_name(group)?;

    let start = start.filter(|s| !s.trim().is_empty()).unwrap_or("-");
    let end = end.filter(|s| !s.trim().is_empty()).unwrap_or("+");
    let count = count.unwrap_or(100).max(1);

    let mut cmd = redis::cmd("XPENDING");
    cmd.arg(key).arg(&group).arg(start).arg(end).arg(count);
    if let Some(c) = consumer.filter(|s| !s.trim().is_empty()) {
        cmd.arg(c.trim());
    }

    let raw: redis::Value = cmd.query_async(conn).await.map_err(|e| e.to_string())?;
    parse_xpending_entries(&raw)
}

pub async fn xack<C>(conn: &mut C, key: &str, group: &str, ids: &[String]) -> Result<u64, String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    let key = key.trim();
    if key.is_empty() {
        return Err("key is required".into());
    }
    let group = validate_xgroup_name(group)?;
    let ids: Vec<&str> = ids
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    if ids.is_empty() {
        return Err("at least one entry id is required".into());
    }

    let mut cmd = redis::cmd("XACK");
    cmd.arg(key).arg(group);
    for id in ids {
        cmd.arg(id);
    }

    let acked: u64 = cmd.query_async(conn).await.map_err(|e| e.to_string())?;
    Ok(acked)
}

pub async fn stream_overview<C>(conn: &mut C, limit: u32) -> Result<StreamOverviewResult, String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    let limit = limit.max(1) as usize;
    let db_size: u64 = redis::cmd("DBSIZE")
        .query_async(conn)
        .await
        .map_err(|e| e.to_string())?;

    let mut stream_keys = Vec::new();
    let mut cursor = 0u64;
    let scan_exhausted = 'scan: {
        loop {
            let raw: redis::Value = redis::cmd("SCAN")
                .arg(cursor)
                .arg("COUNT")
                .arg(200)
                .query_async(conn)
                .await
                .map_err(|e| e.to_string())?;
            let (next, batch) = parse_scan_result(&raw);
            if batch.is_empty() && next == 0 {
                break 'scan true;
            }

            let mut pipe = redis::pipe();
            for key in &batch {
                pipe.cmd("TYPE").arg(key);
            }
            let types: Vec<redis::Value> =
                pipe.query_async(conn).await.map_err(|e| e.to_string())?;

            for (key, ty) in batch.into_iter().zip(types.into_iter()) {
                if value_to_string(&ty).eq_ignore_ascii_case("stream") {
                    stream_keys.push(key);
                    if stream_keys.len() >= limit {
                        break;
                    }
                }
            }

            if stream_keys.len() >= limit {
                break 'scan cursor == 0;
            }
            cursor = next;
            if cursor == 0 {
                break 'scan true;
            }
        }
    };

    let mut rows = Vec::with_capacity(stream_keys.len());
    for key in stream_keys {
        let length: u64 = redis::cmd("XLEN")
            .arg(&key)
            .query_async(conn)
            .await
            .map_err(|e| e.to_string())?;

        let groups_raw: redis::Value = redis::cmd("XINFO")
            .arg("GROUPS")
            .arg(&key)
            .query_async(conn)
            .await
            .map_err(|e| e.to_string())?;
        let groups = parse_xinfo_groups(&groups_raw)?;
        let pending_total = groups.iter().map(|g| g.pending).sum();

        rows.push(StreamOverviewRow {
            key,
            length,
            group_count: groups.len() as u64,
            pending_total,
        });
    }

    rows.sort_by(|a, b| {
        b.pending_total
            .cmp(&a.pending_total)
            .then_with(|| b.length.cmp(&a.length))
            .then_with(|| a.key.cmp(&b.key))
    });

    let truncated = !scan_exhausted || db_size > rows.len() as u64;
    Ok(StreamOverviewResult { rows, truncated })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_xgroup_name_rejects_empty() {
        assert!(validate_xgroup_name("").is_err());
        assert!(validate_xgroup_name("   ").is_err());
    }

    #[test]
    fn validate_xgroup_name_trims_and_accepts() {
        assert_eq!(validate_xgroup_name("  workers  ").unwrap(), "workers");
    }

    #[test]
    fn resolve_stream_overview_limit_defaults() {
        assert_eq!(resolve_stream_overview_limit(None), 100);
        assert_eq!(resolve_stream_overview_limit(Some(0)), 100);
        assert_eq!(resolve_stream_overview_limit(Some(25)), 25);
    }

    #[test]
    fn parse_stream_entry_basic() {
        let raw = redis::Value::Array(vec![
            redis::Value::BulkString(b"1000-0".to_vec()),
            redis::Value::Array(vec![
                redis::Value::BulkString(b"field".to_vec()),
                redis::Value::BulkString(b"value".to_vec()),
            ]),
        ]);
        let entry = parse_stream_entry(&raw).unwrap();
        assert_eq!(entry.id, "1000-0");
        assert_eq!(entry.fields.get("field"), Some(&"value".to_string()));
    }

    #[test]
    fn parse_xinfo_groups_from_array_pairs() {
        let raw = redis::Value::Array(vec![redis::Value::Array(vec![
            redis::Value::BulkString(b"name".to_vec()),
            redis::Value::BulkString(b"g1".to_vec()),
            redis::Value::BulkString(b"consumers".to_vec()),
            redis::Value::Int(2),
            redis::Value::BulkString(b"pending".to_vec()),
            redis::Value::Int(3),
            redis::Value::BulkString(b"last-delivered-id".to_vec()),
            redis::Value::BulkString(b"1000-0".to_vec()),
        ])]);
        let groups = parse_xinfo_groups(&raw).unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].name, "g1");
        assert_eq!(groups[0].consumers, 2);
        assert_eq!(groups[0].pending, 3);
        assert_eq!(groups[0].last_delivered_id, "1000-0");
    }

    #[test]
    fn parse_xpending_entries_from_detail_rows() {
        let raw = redis::Value::Array(vec![redis::Value::Array(vec![
            redis::Value::BulkString(b"1000-0".to_vec()),
            redis::Value::BulkString(b"c1".to_vec()),
            redis::Value::Int(42),
            redis::Value::Int(1),
        ])]);
        let pending = parse_xpending_entries(&raw).unwrap();
        assert_eq!(pending.entries.len(), 1);
        assert_eq!(pending.entries[0].id, "1000-0");
        assert_eq!(pending.entries[0].consumer, "c1");
        assert_eq!(pending.entries[0].idle_ms, 42);
        assert_eq!(pending.entries[0].delivery_count, 1);
    }
}
