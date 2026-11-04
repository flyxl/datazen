//! Consumer-group commands: `XINFO GROUPS` / `XINFO CONSUMERS`, `XPENDING`, `XACK`, and the lag computation built on them.

use super::parse::parse_xinfo_consumers;
use super::parse::parse_xinfo_groups;
use super::parse::parse_xpending_entries;
use super::types::validate_xgroup_name;
use super::types::ConsumerInfo;
use super::types::StreamGroupInfo;
use super::types::XpendingResult;
use redis::AsyncCommands;

pub async fn xinfo_consumers<C>(
    conn: &mut C,
    key: &str,
    group: &str,
) -> Result<Vec<ConsumerInfo>, String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    let key = key.trim();
    if key.is_empty() {
        return Err("key is required".into());
    }
    let group = validate_xgroup_name(group)?;

    let raw: redis::Value = redis::cmd("XINFO")
        .arg("CONSUMERS")
        .arg(key)
        .arg(&group)
        .query_async(conn)
        .await
        .map_err(|e| e.to_string())?;

    parse_xinfo_consumers(&raw)
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
