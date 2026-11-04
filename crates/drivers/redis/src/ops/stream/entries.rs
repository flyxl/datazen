//! Entry-level commands: `XRANGE`, `XADD`, `XGROUP CREATE|DESTROY`.

use super::parse::parse_stream_entry;
use super::parse::parse_stream_id;
use super::parse::parse_xinfo_groups;
use super::parse::value_to_string;
use super::types::validate_xgroup_name;
use super::types::StreamLagResult;
use super::types::XaddResult;
use super::types::XrangeResult;
use redis::AsyncCommands;
use std::collections::HashMap;

pub async fn stream_lag<C>(conn: &mut C, key: &str, group: &str) -> Result<StreamLagResult, String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    let key = key.trim();
    if key.is_empty() {
        return Err("key is required".into());
    }
    let group = validate_xgroup_name(group)?;

    // Fetch XINFO GROUPS to get last-delivered-id
    let groups_raw: redis::Value = redis::cmd("XINFO")
        .arg("GROUPS")
        .arg(key)
        .query_async(conn)
        .await
        .map_err(|e| e.to_string())?;
    let groups = parse_xinfo_groups(&groups_raw)?;

    let group_info = groups
        .iter()
        .find(|g| g.name == group)
        .ok_or_else(|| format!("consumer group '{}' not found", group))?;

    // If no entries delivered yet, lag = stream length (all entries are pending)
    if group_info.last_delivered_id.is_empty() || group_info.last_delivered_id == "0-0" {
        // Get stream length
        let len: u64 = redis::cmd("XLEN")
            .arg(key)
            .query_async(conn)
            .await
            .map_err(|e| e.to_string())?;
        return Ok(StreamLagResult { lag: Some(len) });
    }

    // Get the max ID in the stream using XRANGE with COUNT 1 from the end
    let max_raw: redis::Value = redis::cmd("XREVRANGE")
        .arg(key)
        .arg("+")
        .arg("-")
        .arg("COUNT")
        .arg(1u32)
        .query_async(conn)
        .await
        .map_err(|e| e.to_string())?;

    let max_entries = match &max_raw {
        redis::Value::Array(items) => items,
        _ => return Ok(StreamLagResult { lag: None }),
    };

    if max_entries.is_empty() {
        return Ok(StreamLagResult { lag: None });
    }

    // Extract the max entry ID
    let max_id_str = match &max_entries[0] {
        redis::Value::Array(parts) if !parts.is_empty() => value_to_string(&parts[0]),
        _ => return Ok(StreamLagResult { lag: None }),
    };

    let (max_ms, max_seq) =
        parse_stream_id(&max_id_str).ok_or_else(|| format!("invalid stream ID: {}", max_id_str))?;
    let (delivered_ms, delivered_seq) =
        parse_stream_id(&group_info.last_delivered_id).ok_or_else(|| {
            format!(
                "invalid last-delivered-id: {}",
                group_info.last_delivered_id
            )
        })?;

    // lag = max_id - last_delivered_id (approximate, using milliseconds-level comparison)
    let lag = if max_ms > delivered_ms {
        Some(max_ms - delivered_ms)
    } else if max_ms == delivered_ms && max_seq > delivered_seq {
        Some(max_seq - delivered_seq)
    } else {
        Some(0)
    };

    Ok(StreamLagResult { lag })
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
