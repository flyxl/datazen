//! INFO / MEMORY / Slowlog helpers for Redis Monitor plugin commands.

use redis::AsyncCommands;
use redis::FromRedisValue;
use serde::Serialize;

use crate::redis_driver::parse_scan_result;

/// Default number of keys to sample when `limit` is omitted or zero.
pub const DEFAULT_MEMORY_SAMPLE_LIMIT: u32 = 200;

/// Resolve the effective memory sample limit (defaults to [`DEFAULT_MEMORY_SAMPLE_LIMIT`]).
pub fn resolve_memory_sample_limit(limit: Option<u32>) -> u32 {
    match limit {
        Some(0) | None => DEFAULT_MEMORY_SAMPLE_LIMIT,
        Some(n) => n,
    }
}

/// Parse Redis `INFO` text into `(section_name, [(key, value), ...])` pairs.
#[allow(dead_code)] // used by upcoming Redis INFO / monitor views
pub fn parse_info_sections(raw: &str) -> Vec<(String, Vec<(String, String)>)> {
    let mut sections: Vec<(String, Vec<(String, String)>)> = Vec::new();
    let mut current: Option<(String, Vec<(String, String)>)> = None;

    for line in raw.split(['\n', '\r']).filter(|l| !l.is_empty()) {
        if let Some(name) = line.strip_prefix("# ") {
            if let Some(section) = current.take() {
                sections.push(section);
            }
            current = Some((name.trim().to_string(), Vec::new()));
            continue;
        }
        if let Some((_, ref mut pairs)) = current {
            if let Some((key, value)) = line.split_once(':') {
                pairs.push((key.trim().to_string(), value.trim().to_string()));
            }
        }
    }

    if let Some(section) = current {
        sections.push(section);
    }

    sections
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemorySample {
    pub key: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemorySampleResult {
    pub samples: Vec<MemorySample>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SlowlogEntry {
    pub id: u64,
    pub timestamp: i64,
    pub duration_us: u64,
    pub command: Vec<String>,
    pub client_addr: Option<String>,
    pub client_name: Option<String>,
}

pub async fn fetch_info<C>(
    conn: &mut C,
    section: Option<&str>,
) -> Result<String, String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    let mut cmd = redis::cmd("INFO");
    if let Some(sec) = section.filter(|s| !s.is_empty()) {
        cmd.arg(sec);
    }
    cmd.query_async(conn)
        .await
        .map_err(|e| e.to_string())
}

pub async fn memory_sample<C>(
    conn: &mut C,
    limit: u32,
) -> Result<MemorySampleResult, String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    let limit = limit.max(1) as usize;
    let db_size: u64 = redis::cmd("DBSIZE")
        .query_async(conn)
        .await
        .map_err(|e| e.to_string())?;

    let mut keys = Vec::new();
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
            for key in batch {
                keys.push(key);
                if keys.len() >= limit {
                    break;
                }
            }
            cursor = next;
            if keys.len() >= limit {
                break 'scan cursor == 0;
            }
            if cursor == 0 {
                break 'scan true;
            }
        }
    };

    let mut samples = Vec::with_capacity(keys.len());
    for key in keys {
        let bytes: Option<u64> = redis::cmd("MEMORY")
            .arg("USAGE")
            .arg(&key)
            .query_async(conn)
            .await
            .map_err(|e| e.to_string())?;
        samples.push(MemorySample {
            key,
            bytes: bytes.unwrap_or(0),
        });
    }

    samples.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.key.cmp(&b.key)));

    let truncated = !scan_exhausted || db_size > samples.len() as u64;
    Ok(MemorySampleResult {
        samples,
        truncated,
    })
}

pub async fn slowlog_get<C>(
    conn: &mut C,
    count: u32,
) -> Result<Vec<SlowlogEntry>, String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    let count = count.max(1);
    let raw: redis::Value = redis::cmd("SLOWLOG")
        .arg("GET")
        .arg(count)
        .query_async(conn)
        .await
        .map_err(|e| e.to_string())?;
    parse_slowlog_entries(&raw)
}

pub async fn slowlog_reset<C>(conn: &mut C) -> Result<(), String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    redis::cmd("SLOWLOG")
        .arg("RESET")
        .query_async::<()>(conn)
        .await
        .map_err(|e| e.to_string())
}

pub async fn modules_list<C>(conn: &mut C) -> Result<Vec<String>, String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    let raw: redis::Value = redis::cmd("MODULE")
        .arg("LIST")
        .query_async(conn)
        .await
        .map_err(|e| e.to_string())?;
    parse_module_names(&raw)
}

/// Reject slowlog reset unless the frontend passes `confirm: true`.
pub fn ensure_slowlog_reset_confirmed(confirm: bool) -> Result<(), String> {
    if !confirm {
        return Err("Slowlog reset requires confirm=true".into());
    }
    Ok(())
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

fn parse_slowlog_entries(raw: &redis::Value) -> Result<Vec<SlowlogEntry>, String> {
    let items = match raw {
        redis::Value::Array(items) => items,
        _ => return Ok(vec![]),
    };

    items
        .iter()
        .filter_map(|entry| parse_slowlog_entry(entry).transpose())
        .collect()
}

fn parse_slowlog_entry(raw: &redis::Value) -> Result<Option<SlowlogEntry>, String> {
    let parts = match raw {
        redis::Value::Array(items) => items,
        _ => return Ok(None),
    };
    if parts.len() < 4 {
        return Ok(None);
    }

    let id: u64 = FromRedisValue::from_redis_value(&parts[0]).map_err(|e| e.to_string())?;
    let timestamp: i64 = FromRedisValue::from_redis_value(&parts[1]).map_err(|e| e.to_string())?;
    let duration_us: u64 =
        FromRedisValue::from_redis_value(&parts[2]).map_err(|e| e.to_string())?;

    let command = match &parts[3] {
        redis::Value::Array(args) => args.iter().map(value_to_string).collect(),
        other => vec![value_to_string(other)],
    };

    let client_addr = parts
        .get(4)
        .filter(|v| !matches!(v, redis::Value::Nil))
        .map(value_to_string)
        .filter(|s| !s.is_empty());
    let client_name = parts
        .get(5)
        .filter(|v| !matches!(v, redis::Value::Nil))
        .map(value_to_string)
        .filter(|s| !s.is_empty());

    Ok(Some(SlowlogEntry {
        id,
        timestamp,
        duration_us,
        command,
        client_addr,
        client_name,
    }))
}

fn parse_module_names(raw: &redis::Value) -> Result<Vec<String>, String> {
    let modules = match raw {
        redis::Value::Array(items) => items,
        _ => return Ok(vec![]),
    };

    let mut names = Vec::new();
    for module in modules {
        match module {
            redis::Value::Map(pairs) => {
                for (key, value) in pairs {
                    if value_to_string(key).eq_ignore_ascii_case("name") {
                        let name = value_to_string(value);
                        if !name.is_empty() {
                            names.push(name);
                        }
                    }
                }
            }
            redis::Value::Array(fields) => {
                for chunk in fields.chunks(2) {
                    if chunk.len() == 2 && value_to_string(&chunk[0]).eq_ignore_ascii_case("name") {
                        let name = value_to_string(&chunk[1]);
                        if !name.is_empty() {
                            names.push(name);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    names.sort();
    names.dedup();
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_info_sections_splits_headers() {
        let raw = "# Server\r\nredis_version:7.0.0\r\n# Memory\r\nused_memory:100\r\n";
        let sections = parse_info_sections(raw);
        assert_eq!(sections[0].0, "Server");
        assert_eq!(sections[0].1[0].0, "redis_version");
        assert_eq!(sections[0].1[0].1, "7.0.0");
        assert_eq!(sections[1].0, "Memory");
        assert_eq!(sections[1].1[0].0, "used_memory");
        assert_eq!(sections[1].1[0].1, "100");
    }

    #[test]
    fn parse_info_sections_handles_lf_only() {
        let raw = "# Server\nredis_version:7.0.0\n";
        let sections = parse_info_sections(raw);
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].0, "Server");
    }

    #[test]
    fn resolve_memory_sample_limit_defaults() {
        assert_eq!(resolve_memory_sample_limit(None), 200);
        assert_eq!(resolve_memory_sample_limit(Some(0)), 200);
        assert_eq!(resolve_memory_sample_limit(Some(50)), 50);
    }

    #[test]
    fn slowlog_reset_gate_rejects_without_confirm() {
        let err = ensure_slowlog_reset_confirmed(false).unwrap_err();
        assert!(err.contains("confirm"));
        assert!(ensure_slowlog_reset_confirmed(true).is_ok());
    }

    #[test]
    fn parse_slowlog_entry_basic() {
        let raw = redis::Value::Array(vec![
            redis::Value::Int(14),
            redis::Value::Int(1_309_448_223),
            redis::Value::Int(154_739),
            redis::Value::Array(vec![
                redis::Value::BulkString(b"GET".to_vec()),
                redis::Value::BulkString(b"mykey".to_vec()),
            ]),
            redis::Value::BulkString(b"127.0.0.1:59072".to_vec()),
            redis::Value::BulkString(b"client-1".to_vec()),
        ]);
        let entry = parse_slowlog_entry(&raw).unwrap().unwrap();
        assert_eq!(entry.id, 14);
        assert_eq!(entry.command, vec!["GET".to_string(), "mykey".to_string()]);
        assert_eq!(entry.client_addr.as_deref(), Some("127.0.0.1:59072"));
        assert_eq!(entry.client_name.as_deref(), Some("client-1"));
    }

    #[test]
    fn parse_module_names_from_map_entries() {
        let raw = redis::Value::Array(vec![redis::Value::Map(vec![
            (
                redis::Value::BulkString(b"name".to_vec()),
                redis::Value::BulkString(b"ReJSON".to_vec()),
            ),
            (
                redis::Value::BulkString(b"ver".to_vec()),
                redis::Value::Int(9999),
            ),
        ])]);
        let names = parse_module_names(&raw).unwrap();
        assert_eq!(names, vec!["ReJSON".to_string()]);
    }
}
