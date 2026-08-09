//! Pure helpers and Redis mutate/batch operations for plugin commands.

use redis::AsyncCommands;
use serde::Serialize;

use crate::redis_driver::parse_scan_result;

/// TTL `-1` means remove expiry (PERSIST); non-negative values set EXPIRE.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TtlCommand {
    Persist,
    Expire(u64),
}

pub fn resolve_ttl(ttl_seconds: i64) -> Result<TtlCommand, String> {
    match ttl_seconds {
        -1 => Ok(TtlCommand::Persist),
        n if n >= 0 => Ok(TtlCommand::Expire(n as u64)),
        _ => Err(format!("invalid ttl_seconds: {ttl_seconds}")),
    }
}

/// Build `(old_key, new_key)` pairs for keys that start with `old_prefix`.
pub fn plan_rename_prefix(
    old_prefix: &str,
    new_prefix: &str,
    keys: &[String],
) -> Vec<(String, String)> {
    keys.iter()
        .filter(|k| k.starts_with(old_prefix))
        .map(|k| {
            let suffix = &k[old_prefix.len()..];
            (k.clone(), format!("{new_prefix}{suffix}"))
        })
        .collect()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyError {
    pub key: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchDeleteResult {
    pub deleted: u64,
    pub errors: Vec<KeyError>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchSetTtlResult {
    pub updated: u64,
    pub errors: Vec<KeyError>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchRenameResult {
    pub renamed: u64,
    pub errors: Vec<KeyError>,
}

pub async fn scan_matching_keys<C>(
    conn: &mut C,
    pattern: &str,
) -> Result<Vec<String>, String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    let mut keys = Vec::new();
    let mut cursor = 0u64;
    loop {
        let raw: redis::Value = redis::cmd("SCAN")
            .arg(cursor)
            .arg("MATCH")
            .arg(pattern)
            .arg("COUNT")
            .arg(200)
            .query_async(conn)
            .await
            .map_err(|e| e.to_string())?;
        let (next, batch) = parse_scan_result(&raw);
        keys.extend(batch);
        cursor = next;
        if cursor == 0 {
            break;
        }
    }
    Ok(keys)
}

pub async fn count_matching<C>(
    conn: &mut C,
    pattern: &str,
) -> Result<u64, String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    let keys = scan_matching_keys(conn, pattern).await?;
    Ok(keys.len() as u64)
}

pub async fn set_string<C>(
    conn: &mut C,
    key: &str,
    value: &str,
) -> Result<(), String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    conn.set(key, value)
        .await
        .map_err(|e| e.to_string())
}

pub async fn hash_set<C>(
    conn: &mut C,
    key: &str,
    field: &str,
    value: &str,
) -> Result<(), String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    conn.hset(key, field, value)
        .await
        .map_err(|e| e.to_string())
}

pub async fn hash_del<C>(
    conn: &mut C,
    key: &str,
    fields: &[String],
) -> Result<(), String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    if fields.is_empty() {
        return Ok(());
    }
    conn.hdel::<_, _, i64>(key, fields)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

pub async fn list_push<C>(
    conn: &mut C,
    key: &str,
    side: &str,
    values: &[String],
) -> Result<(), String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    if values.is_empty() {
        return Ok(());
    }
    match side {
        "left" => conn
            .lpush::<_, _, i64>(key, values)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string()),
        "right" => conn
            .rpush::<_, _, i64>(key, values)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string()),
        _ => Err(format!("invalid side: {side} (expected \"left\" or \"right\")")),
    }
}

pub async fn list_set<C>(
    conn: &mut C,
    key: &str,
    index: i64,
    value: &str,
) -> Result<(), String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    redis::cmd("LSET")
        .arg(key)
        .arg(index)
        .arg(value)
        .query_async::<()>(conn)
        .await
        .map_err(|e| e.to_string())
}

pub async fn list_pop<C>(
    conn: &mut C,
    key: &str,
    side: &str,
) -> Result<Option<String>, String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    let raw: redis::Value = match side {
        "left" => redis::cmd("LPOP").arg(key).query_async(conn).await,
        "right" => redis::cmd("RPOP").arg(key).query_async(conn).await,
        _ => return Err(format!("invalid side: {side} (expected \"left\" or \"right\")")),
    }
    .map_err(|e| e.to_string())?;
    Ok(match raw {
        redis::Value::Nil => None,
        redis::Value::BulkString(b) => Some(String::from_utf8_lossy(&b).into()),
        redis::Value::SimpleString(s) => Some(s),
        other => Some(format!("{other:?}")),
    })
}

pub async fn set_add<C>(
    conn: &mut C,
    key: &str,
    members: &[String],
) -> Result<(), String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    if members.is_empty() {
        return Ok(());
    }
    conn.sadd::<_, _, i64>(key, members)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

pub async fn set_remove<C>(
    conn: &mut C,
    key: &str,
    members: &[String],
) -> Result<(), String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    if members.is_empty() {
        return Ok(());
    }
    conn.srem::<_, _, i64>(key, members)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ZsetMember {
    pub member: String,
    pub score: f64,
}

pub async fn zset_add<C>(
    conn: &mut C,
    key: &str,
    members: &[ZsetMember],
) -> Result<(), String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    if members.is_empty() {
        return Ok(());
    }
    let mut pipe = redis::pipe();
    for m in members {
        pipe.cmd("ZADD").arg(key).arg(m.score).arg(&m.member);
    }
    pipe.query_async::<()>(conn)
        .await
        .map_err(|e| e.to_string())
}

pub async fn zset_remove<C>(
    conn: &mut C,
    key: &str,
    members: &[String],
) -> Result<(), String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    if members.is_empty() {
        return Ok(());
    }
    conn.zrem::<_, _, i64>(key, members)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

pub async fn delete_keys<C>(
    conn: &mut C,
    keys: &[String],
) -> Result<u64, String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    if keys.is_empty() {
        return Ok(0);
    }
    conn.del(keys)
        .await
        .map_err(|e| e.to_string())
}

pub async fn rename_key<C>(
    conn: &mut C,
    key: &str,
    new_key: &str,
) -> Result<(), String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    redis::cmd("RENAME")
        .arg(key)
        .arg(new_key)
        .query_async::<()>(conn)
        .await
        .map_err(|e| e.to_string())
}

pub async fn set_ttl<C>(
    conn: &mut C,
    key: &str,
    ttl_seconds: i64,
) -> Result<(), String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    match resolve_ttl(ttl_seconds)? {
        TtlCommand::Persist => conn
            .persist::<_, i64>(key)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string()),
        TtlCommand::Expire(secs) => conn
            .expire::<_, i64>(key, secs as i64)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string()),
    }
}

pub async fn batch_delete_pattern<C>(
    conn: &mut C,
    pattern: &str,
) -> Result<BatchDeleteResult, String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    let keys = scan_matching_keys(conn, pattern).await?;
    let mut deleted = 0u64;
    let mut errors = Vec::new();
    for key in keys {
        match conn.del::<_, u64>(&key).await {
            Ok(n) => deleted += n,
            Err(e) => errors.push(KeyError {
                key,
                error: e.to_string(),
            }),
        }
    }
    Ok(BatchDeleteResult { deleted, errors })
}

pub async fn batch_set_ttl<C>(
    conn: &mut C,
    keys: &[String],
    ttl_seconds: i64,
) -> Result<BatchSetTtlResult, String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    let cmd = resolve_ttl(ttl_seconds)?;
    let mut updated = 0u64;
    let mut errors = Vec::new();
    for key in keys {
        let result = match cmd {
            TtlCommand::Persist => conn.persist::<_, i64>(key).await,
            TtlCommand::Expire(secs) => conn.expire::<_, i64>(key, secs as i64).await,
        };
        match result {
            Ok(1) => updated += 1,
            Ok(_) => errors.push(KeyError {
                key: key.clone(),
                error: "key does not exist".into(),
            }),
            Err(e) => errors.push(KeyError {
                key: key.clone(),
                error: e.to_string(),
            }),
        }
    }
    Ok(BatchSetTtlResult { updated, errors })
}

pub async fn batch_rename_prefix<C>(
    conn: &mut C,
    old_prefix: &str,
    new_prefix: &str,
    keys: Option<Vec<String>>,
) -> Result<BatchRenameResult, String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    let source_keys = match keys {
        Some(k) => k,
        None => scan_matching_keys(conn, &format!("{old_prefix}*")).await?,
    };
    let planned = plan_rename_prefix(old_prefix, new_prefix, &source_keys);
    let mut renamed = 0u64;
    let mut errors = Vec::new();
    for (old_key, new_key) in planned {
        match rename_key(conn, &old_key, &new_key).await {
            Ok(()) => renamed += 1,
            Err(e) => errors.push(KeyError {
                key: old_key,
                error: e,
            }),
        }
    }
    Ok(BatchRenameResult { renamed, errors })
}

pub async fn flush_db<C>(conn: &mut C) -> Result<(), String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    redis::cmd("FLUSHDB")
        .query_async::<()>(conn)
        .await
        .map_err(|e| e.to_string())
}

pub async fn flush_all<C>(conn: &mut C) -> Result<(), String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    redis::cmd("FLUSHALL")
        .query_async::<()>(conn)
        .await
        .map_err(|e| e.to_string())
}

/// Reject destructive flush commands unless the frontend passes `allow_flush: true`.
pub fn ensure_flush_allowed(allow_flush: bool) -> Result<(), String> {
    if !allow_flush {
        return Err("Flush is disabled in Redis extension settings".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_rename_prefix_rewrites() {
        let planned = plan_rename_prefix("user:", "u:", &["user:1".into(), "user:2".into()]);
        assert_eq!(
            planned,
            vec![("user:1".into(), "u:1".into()), ("user:2".into(), "u:2".into())]
        );
    }

    #[test]
    fn plan_rename_prefix_skips_non_matching() {
        let planned = plan_rename_prefix("user:", "u:", &["other:1".into(), "user:x".into()]);
        assert_eq!(planned, vec![("user:x".into(), "u:x".into())]);
    }

    #[test]
    fn ttl_sentinel_persist() {
        assert_eq!(resolve_ttl(-1).unwrap(), TtlCommand::Persist);
    }

    #[test]
    fn ttl_sentinel_expire() {
        assert_eq!(resolve_ttl(3600).unwrap(), TtlCommand::Expire(3600));
    }

    #[test]
    fn ttl_sentinel_rejects_invalid() {
        assert!(resolve_ttl(-2).is_err());
    }

    #[test]
    fn flush_gate_rejects_when_disabled() {
        let err = ensure_flush_allowed(false).unwrap_err();
        assert!(err.contains("Flush is disabled"));
        assert!(ensure_flush_allowed(true).is_ok());
    }
}
