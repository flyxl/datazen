//! Pure helpers and Redis mutate/batch operations for plugin commands.

use redis::AsyncCommands;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::redis_driver::parse_scan_result;

/// TTL resolution for relative seconds, absolute unix expiry, or persist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TtlCommand {
    /// Remove expiry (PERSIST).
    Persist,
    /// Relative expiry via EXPIRE (seconds from now).
    Expire(u64),
    /// Absolute expiry via EXPIREAT (unix timestamp seconds).
    ExpireAt(i64),
}

pub fn resolve_ttl(ttl_seconds: i64) -> Result<TtlCommand, String> {
    match ttl_seconds {
        -1 => Ok(TtlCommand::Persist),
        n if n >= 0 => Ok(TtlCommand::Expire(n as u64)),
        _ => Err(format!("invalid ttl_seconds: {ttl_seconds}")),
    }
}

/// Resolve absolute expire-at unix timestamp (must be > 0).
pub fn resolve_expire_at(expire_at: i64) -> Result<TtlCommand, String> {
    if expire_at <= 0 {
        return Err(format!(
            "invalid expire_at: {expire_at} (expected unix timestamp > 0)"
        ));
    }
    Ok(TtlCommand::ExpireAt(expire_at))
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

/// Issue one SCAN round-trip: `SCAN cursor COUNT n [MATCH p] [TYPE t]`.
///
/// Shared by the flat key browser (`scan_keys`, `count_matching`), the
/// hierarchical `list_children`, and the guarded value search; each caller
/// applies its own post-processing of the returned key batch.
pub(crate) async fn scan_batch<C>(
    conn: &mut C,
    cursor: u64,
    count: u32,
    pattern: Option<&str>,
    type_filter: Option<&str>,
) -> Result<(u64, Vec<String>), String>
where
    C: redis::aio::ConnectionLike + Send,
{
    let mut cmd = redis::cmd("SCAN");
    cmd.arg(cursor).arg("COUNT").arg(count.max(1));
    if let Some(p) = pattern {
        cmd.arg("MATCH").arg(p);
    }
    if let Some(t) = type_filter {
        cmd.arg("TYPE").arg(t);
    }
    let raw: redis::Value = cmd.query_async(conn).await.map_err(|e| e.to_string())?;
    Ok(parse_scan_result(&raw))
}

/// Shared SCAN loop used by key browser, pattern deletes, and counts.
pub async fn scan_keys<C>(
    conn: &mut C,
    pattern: Option<&str>,
    max_keys: Option<usize>,
) -> Result<Vec<String>, String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    let mut keys = Vec::new();
    let mut cursor = 0u64;
    loop {
        let (next, batch) = scan_batch(conn, cursor, 200, pattern, None).await?;
        keys.extend(batch);
        cursor = next;
        if cursor == 0 {
            break;
        }
        if let Some(max) = max_keys {
            if keys.len() >= max {
                keys.truncate(max);
                break;
            }
        }
    }
    Ok(keys)
}

pub async fn scan_matching_keys<C>(conn: &mut C, pattern: &str) -> Result<Vec<String>, String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    scan_keys(conn, Some(pattern), None).await
}

pub async fn count_matching<C>(conn: &mut C, pattern: &str) -> Result<u64, String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    if pattern == "*" {
        let dbsize: i64 = redis::cmd("DBSIZE")
            .query_async(conn)
            .await
            .map_err(|e| e.to_string())?;
        return Ok(dbsize.max(0) as u64);
    }
    let mut total = 0u64;
    let mut cursor = 0u64;
    loop {
        let (next, batch) = scan_batch(conn, cursor, 200, Some(pattern), None).await?;
        total += batch.len() as u64;
        cursor = next;
        if cursor == 0 {
            break;
        }
    }
    Ok(total)
}

/// SET a string value. When `keep_ttl` is true, uses Redis `SET … KEEPTTL`
/// so an existing expiry is preserved (Redis ≥ 6.0).
pub async fn set_string_with_options<C>(
    conn: &mut C,
    key: &str,
    value: &str,
    keep_ttl: bool,
) -> Result<(), String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    if keep_ttl {
        redis::cmd("SET")
            .arg(key)
            .arg(value)
            .arg("KEEPTTL")
            .query_async::<()>(conn)
            .await
            .map_err(|e| e.to_string())
    } else {
        conn.set(key, value).await.map_err(|e| e.to_string())
    }
}

pub async fn hash_set<C>(conn: &mut C, key: &str, field: &str, value: &str) -> Result<(), String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    conn.hset(key, field, value)
        .await
        .map_err(|e| e.to_string())
}

pub async fn hash_del<C>(conn: &mut C, key: &str, fields: &[String]) -> Result<(), String>
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
        _ => Err(format!(
            "invalid side: {side} (expected \"left\" or \"right\")"
        )),
    }
}

pub async fn list_set<C>(conn: &mut C, key: &str, index: i64, value: &str) -> Result<(), String>
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

pub async fn list_pop<C>(conn: &mut C, key: &str, side: &str) -> Result<Option<String>, String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    let raw: redis::Value = match side {
        "left" => redis::cmd("LPOP").arg(key).query_async(conn).await,
        "right" => redis::cmd("RPOP").arg(key).query_async(conn).await,
        _ => {
            return Err(format!(
                "invalid side: {side} (expected \"left\" or \"right\")"
            ))
        }
    }
    .map_err(|e| e.to_string())?;
    Ok(match raw {
        redis::Value::Nil => None,
        redis::Value::BulkString(b) => Some(String::from_utf8_lossy(&b).into()),
        redis::Value::SimpleString(s) => Some(s),
        other => Some(format!("{other:?}")),
    })
}

/// LINDEX: get the element at `index` in the list stored at `key`.
pub async fn list_index<C>(conn: &mut C, key: &str, index: i64) -> Result<Option<String>, String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    let raw: redis::Value = redis::cmd("LINDEX")
        .arg(key)
        .arg(index)
        .query_async(conn)
        .await
        .map_err(|e| e.to_string())?;
    Ok(match raw {
        redis::Value::Nil => None,
        redis::Value::BulkString(b) => Some(String::from_utf8_lossy(&b).into()),
        redis::Value::SimpleString(s) => Some(s),
        other => Some(format!("{other:?}")),
    })
}

/// LREM: remove `count` occurrences of `value` from the list stored at `key`.
/// count > 0: remove first `count` occurrences; count < 0: remove last `count`; count = 0: remove all.
pub async fn list_rem<C>(conn: &mut C, key: &str, count: i64, value: &str) -> Result<i64, String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    redis::cmd("LREM")
        .arg(key)
        .arg(count)
        .arg(value)
        .query_async(conn)
        .await
        .map_err(|e| e.to_string())
}

pub async fn set_add<C>(conn: &mut C, key: &str, members: &[String]) -> Result<(), String>
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

pub async fn set_remove<C>(conn: &mut C, key: &str, members: &[String]) -> Result<(), String>
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

pub async fn zset_add<C>(conn: &mut C, key: &str, members: &[ZsetMember]) -> Result<(), String>
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

pub async fn zset_remove<C>(conn: &mut C, key: &str, members: &[String]) -> Result<(), String>
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

// ── Collection scan / range operations (PR-3) ──────────────────────────

/// HSCAN wrapper: returns (next_cursor, Vec<(field, value)>).
pub async fn hash_scan<C>(
    conn: &mut C,
    key: &str,
    cursor: u64,
    count: u32,
    match_pattern: Option<&str>,
) -> Result<(u64, Vec<(String, String)>), String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    let mut cmd = redis::cmd("HSCAN");
    cmd.arg(key).arg(cursor).arg("COUNT").arg(count);
    if let Some(pat) = match_pattern {
        cmd.arg("MATCH").arg(pat);
    }
    let raw: redis::Value = cmd.query_async(conn).await.map_err(|e| e.to_string())?;
    parse_hash_scan_result(&raw)
}

fn parse_hash_scan_result(raw: &redis::Value) -> Result<(u64, Vec<(String, String)>), String> {
    match raw {
        redis::Value::Array(items) if items.len() == 2 => {
            let next_cursor = parse_cursor_from_value(&items[0])?;
            let pairs = parse_flat_string_pairs(&items[1])?;
            Ok((next_cursor, pairs))
        }
        _ => Err(format!("unexpected HSCAN response: {raw:?}")),
    }
}

/// LRANGE wrapper: returns elements from start to stop (inclusive).
pub async fn list_range<C>(
    conn: &mut C,
    key: &str,
    start: i64,
    stop: i64,
) -> Result<Vec<String>, String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    let raw: redis::Value = redis::cmd("LRANGE")
        .arg(key)
        .arg(start)
        .arg(stop)
        .query_async(conn)
        .await
        .map_err(|e| e.to_string())?;
    parse_string_array(&raw)
}

/// SSCAN wrapper: returns (next_cursor, Vec<member>).
pub async fn set_scan<C>(
    conn: &mut C,
    key: &str,
    cursor: u64,
    count: u32,
    match_pattern: Option<&str>,
) -> Result<(u64, Vec<String>), String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    let mut cmd = redis::cmd("SSCAN");
    cmd.arg(key).arg(cursor).arg("COUNT").arg(count);
    if let Some(pat) = match_pattern {
        cmd.arg("MATCH").arg(pat);
    }
    let raw: redis::Value = cmd.query_async(conn).await.map_err(|e| e.to_string())?;
    parse_scan_result_generic(&raw)
}

/// ZSCAN wrapper: returns (next_cursor, Vec<(member, score)>).
pub async fn zset_scan<C>(
    conn: &mut C,
    key: &str,
    cursor: u64,
    count: u32,
    match_pattern: Option<&str>,
) -> Result<(u64, Vec<(String, f64)>), String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    let mut cmd = redis::cmd("ZSCAN");
    cmd.arg(key).arg(cursor).arg("COUNT").arg(count);
    if let Some(pat) = match_pattern {
        cmd.arg("MATCH").arg(pat);
    }
    let raw: redis::Value = cmd.query_async(conn).await.map_err(|e| e.to_string())?;
    parse_zscan_result(&raw)
}

fn parse_cursor_from_value(v: &redis::Value) -> Result<u64, String> {
    match v {
        redis::Value::Int(n) => Ok(*n as u64),
        redis::Value::BulkString(b) => {
            let s = String::from_utf8_lossy(b);
            s.trim()
                .parse::<u64>()
                .map_err(|e| format!("bad cursor: {e}"))
        }
        redis::Value::SimpleString(s) => s
            .trim()
            .parse::<u64>()
            .map_err(|e| format!("bad cursor: {e}")),
        other => Err(format!("unexpected cursor value: {other:?}")),
    }
}

fn parse_flat_string_pairs(v: &redis::Value) -> Result<Vec<(String, String)>, String> {
    let items = match v {
        redis::Value::Array(items) => items,
        _ => return Err(format!("expected bulk array for scan pairs, got {v:?}")),
    };
    let mut pairs = Vec::with_capacity(items.len() / 2);
    for chunk in items.chunks(2) {
        if chunk.len() == 2 {
            let field = value_to_string(&chunk[0]);
            let value = value_to_string(&chunk[1]);
            pairs.push((field, value));
        }
    }
    Ok(pairs)
}

fn parse_flat_string_array(v: &redis::Value) -> Result<Vec<String>, String> {
    let items = match v {
        redis::Value::Array(items) => items,
        _ => return Err(format!("expected bulk array, got {v:?}")),
    };
    Ok(items.iter().map(value_to_string).collect())
}

fn parse_zscan_result(raw: &redis::Value) -> Result<(u64, Vec<(String, f64)>), String> {
    match raw {
        redis::Value::Array(items) if items.len() == 2 => {
            let next_cursor = parse_cursor_from_value(&items[0])?;
            let flat = parse_flat_string_array(&items[1])?;
            let mut members = Vec::with_capacity(flat.len() / 2);
            for chunk in flat.chunks(2) {
                if chunk.len() == 2 {
                    let member = chunk[0].clone();
                    let score = chunk[1].parse::<f64>().unwrap_or(0.0);
                    members.push((member, score));
                }
            }
            Ok((next_cursor, members))
        }
        _ => Err(format!("unexpected ZSCAN response: {raw:?}")),
    }
}

fn parse_scan_result_generic(raw: &redis::Value) -> Result<(u64, Vec<String>), String> {
    match raw {
        redis::Value::Array(items) if items.len() == 2 => {
            let next_cursor = parse_cursor_from_value(&items[0])?;
            let members = parse_flat_string_array(&items[1])?;
            Ok((next_cursor, members))
        }
        _ => Err(format!("unexpected SSCAN response: {raw:?}")),
    }
}

fn parse_string_array(raw: &redis::Value) -> Result<Vec<String>, String> {
    match raw {
        redis::Value::Array(items) => Ok(items.iter().map(value_to_string).collect()),
        _ => Err(format!("expected array for LRANGE, got {raw:?}")),
    }
}

fn value_to_string(v: &redis::Value) -> String {
    match v {
        redis::Value::Nil => String::new(),
        redis::Value::Int(n) => n.to_string(),
        redis::Value::BulkString(b) => String::from_utf8_lossy(b).into(),
        redis::Value::SimpleString(s) => s.clone(),
        redis::Value::Okay => "OK".into(),
        other => format!("{other:?}"),
    }
}

pub async fn delete_keys<C>(conn: &mut C, keys: &[String]) -> Result<u64, String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    if keys.is_empty() {
        return Ok(0);
    }
    conn.del(keys).await.map_err(|e| e.to_string())
}

pub async fn rename_key<C>(conn: &mut C, key: &str, new_key: &str) -> Result<(), String>
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

pub async fn set_ttl<C>(conn: &mut C, key: &str, ttl_seconds: i64) -> Result<(), String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    apply_ttl_command(conn, key, resolve_ttl(ttl_seconds)?).await
}

/// Set absolute expiry via EXPIREAT (unix timestamp seconds).
pub async fn set_expire_at<C>(conn: &mut C, key: &str, expire_at: i64) -> Result<(), String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    apply_ttl_command(conn, key, resolve_expire_at(expire_at)?).await
}

async fn apply_ttl_command<C>(conn: &mut C, key: &str, cmd: TtlCommand) -> Result<(), String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    match cmd {
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
        TtlCommand::ExpireAt(ts) => redis::cmd("EXPIREAT")
            .arg(key)
            .arg(ts)
            .query_async::<i64>(conn)
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
            TtlCommand::ExpireAt(ts) => {
                redis::cmd("EXPIREAT")
                    .arg(key)
                    .arg(ts)
                    .query_async::<i64>(conn)
                    .await
            }
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

/// Host-synced mirror of `AppSettings.pluginSettings.redis.allowFlush`.
static SETTINGS_ALLOW_FLUSH: AtomicBool = AtomicBool::new(false);

/// Called by the host when settings are loaded or saved (feature `driver-redis`).
pub fn set_settings_allow_flush(allow: bool) {
    SETTINGS_ALLOW_FLUSH.store(allow, Ordering::Relaxed);
}

/// Whether settings currently allow flush (for tests / diagnostics).
pub fn settings_allow_flush() -> bool {
    SETTINGS_ALLOW_FLUSH.load(Ordering::Relaxed)
}

/// Reject destructive flush unless **settings** allow it.
/// The IPC `allow_flush` flag is treated as an additional UI confirmation and
/// cannot bypass a disabled settings toggle.
pub fn ensure_flush_allowed(client_allow_flush: bool) -> Result<(), String> {
    if !SETTINGS_ALLOW_FLUSH.load(Ordering::Relaxed) {
        return Err("Flush is disabled in Redis extension settings".into());
    }
    if !client_allow_flush {
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
            vec![
                ("user:1".into(), "u:1".into()),
                ("user:2".into(), "u:2".into())
            ]
        );
    }

    #[test]
    fn plan_rename_prefix_skips_non_matching() {
        let planned = plan_rename_prefix("user:", "u:", &["other:1".into(), "user:x".into()]);
        assert_eq!(planned, vec![("user:x".into(), "u:x".into())]);
    }

    #[test]
    fn plan_rename_prefix_empty_keys() {
        assert!(plan_rename_prefix("a:", "b:", &[]).is_empty());
    }

    #[test]
    fn plan_rename_prefix_empty_old_prefix_matches_all() {
        let planned = plan_rename_prefix("", "pre:", &["a".into(), "b".into()]);
        assert_eq!(
            planned,
            vec![("a".into(), "pre:a".into()), ("b".into(), "pre:b".into())]
        );
    }

    // ---- PR-1: resolve_ttl / resolve_expire_at ----

    #[test]
    fn ttl_sentinel_persist() {
        assert_eq!(resolve_ttl(-1).unwrap(), TtlCommand::Persist);
    }

    #[test]
    fn ttl_sentinel_expire() {
        assert_eq!(resolve_ttl(3600).unwrap(), TtlCommand::Expire(3600));
    }

    #[test]
    fn ttl_zero_is_expire_zero() {
        assert_eq!(resolve_ttl(0).unwrap(), TtlCommand::Expire(0));
    }

    #[test]
    fn ttl_large_value() {
        assert_eq!(
            resolve_ttl(i64::from(u32::MAX)).unwrap(),
            TtlCommand::Expire(u32::MAX as u64)
        );
    }

    #[test]
    fn ttl_sentinel_rejects_invalid() {
        assert!(resolve_ttl(-2).is_err());
        assert!(resolve_ttl(-100).is_err());
        let err = resolve_ttl(-2).unwrap_err();
        assert!(err.contains("invalid ttl_seconds"), "err={err}");
    }

    #[test]
    fn expire_at_resolves() {
        assert_eq!(
            resolve_expire_at(1_700_000_000).unwrap(),
            TtlCommand::ExpireAt(1_700_000_000)
        );
        assert!(resolve_expire_at(0).is_err());
        assert!(resolve_expire_at(-1).is_err());
    }

    #[test]
    fn expire_at_rejects_zero_and_negative_with_message() {
        let err0 = resolve_expire_at(0).unwrap_err();
        assert!(err0.contains("expire_at"), "err0={err0}");
        assert!(err0.contains("0"), "err0={err0}");
        let err_neg = resolve_expire_at(-5).unwrap_err();
        assert!(err_neg.contains("invalid expire_at"), "err_neg={err_neg}");
    }

    #[test]
    fn expire_at_accepts_far_future() {
        let ts = 4_102_444_800_i64;
        assert_eq!(resolve_expire_at(ts).unwrap(), TtlCommand::ExpireAt(ts));
    }

    #[test]
    fn expire_at_accepts_one() {
        assert_eq!(resolve_expire_at(1).unwrap(), TtlCommand::ExpireAt(1));
    }

    #[test]
    fn ttl_command_variants_are_distinct() {
        assert_ne!(TtlCommand::Persist, TtlCommand::Expire(0));
        assert_ne!(TtlCommand::Expire(1), TtlCommand::ExpireAt(1));
        assert_eq!(TtlCommand::ExpireAt(42), TtlCommand::ExpireAt(42));
    }

    #[test]
    fn flush_gate_requires_settings_and_client_flag() {
        set_settings_allow_flush(false);
        assert!(ensure_flush_allowed(true).is_err());
        assert!(ensure_flush_allowed(false).is_err());

        set_settings_allow_flush(true);
        assert!(ensure_flush_allowed(false).is_err());
        assert!(ensure_flush_allowed(true).is_ok());

        set_settings_allow_flush(false);
    }

    // ---- PR-3: Collection scan / range parse helpers (tester) ----

    // -- value_to_string --

    #[test]
    fn test_tester_value_to_string_nil() {
        assert_eq!(value_to_string(&redis::Value::Nil), "");
    }

    #[test]
    fn test_tester_value_to_string_int() {
        assert_eq!(value_to_string(&redis::Value::Int(42)), "42");
    }

    #[test]
    fn test_tester_value_to_string_bulk_string() {
        let v = redis::Value::BulkString(b"hello".to_vec());
        assert_eq!(value_to_string(&v), "hello");
    }

    #[test]
    fn test_tester_value_to_string_simple_string() {
        assert_eq!(
            value_to_string(&redis::Value::SimpleString("ok".into())),
            "ok"
        );
    }

    #[test]
    fn test_tester_value_to_string_okay() {
        assert_eq!(value_to_string(&redis::Value::Okay), "OK");
    }

    #[test]
    fn test_tester_value_to_string_double_format() {
        // Double is the "other" catch-all => format!("{other:?}")
        let v = redis::Value::Double(3.14);
        let s = value_to_string(&v);
        assert!(s.contains("3.14"), "got: {s}");
    }

    // -- parse_cursor_from_value --

    #[test]
    fn test_tester_cursor_from_int() {
        assert_eq!(parse_cursor_from_value(&redis::Value::Int(0)).unwrap(), 0);
        assert_eq!(
            parse_cursor_from_value(&redis::Value::Int(12345)).unwrap(),
            12345
        );
    }

    #[test]
    fn test_tester_cursor_from_bulk_string() {
        let v = redis::Value::BulkString(b"42".to_vec());
        assert_eq!(parse_cursor_from_value(&v).unwrap(), 42);
    }

    #[test]
    fn test_tester_cursor_from_bulk_string_with_whitespace() {
        let v = redis::Value::BulkString(b"  100  ".to_vec());
        assert_eq!(parse_cursor_from_value(&v).unwrap(), 100);
    }

    #[test]
    fn test_tester_cursor_from_simple_string() {
        let v = redis::Value::SimpleString("7".into());
        assert_eq!(parse_cursor_from_value(&v).unwrap(), 7);
    }

    #[test]
    fn test_tester_cursor_from_invalid_bulk_string() {
        let v = redis::Value::BulkString(b"not_a_number".to_vec());
        assert!(parse_cursor_from_value(&v).is_err());
    }

    #[test]
    fn test_tester_cursor_from_unexpected_type() {
        let v = redis::Value::Array(vec![]);
        assert!(parse_cursor_from_value(&v).is_err());
    }

    // -- parse_flat_string_pairs --

    #[test]
    fn test_tester_flat_pairs_normal() {
        let v = redis::Value::Array(vec![
            redis::Value::BulkString(b"f1".to_vec()),
            redis::Value::BulkString(b"v1".to_vec()),
            redis::Value::BulkString(b"f2".to_vec()),
            redis::Value::BulkString(b"v2".to_vec()),
        ]);
        let pairs = parse_flat_string_pairs(&v).unwrap();
        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[0], ("f1".to_string(), "v1".to_string()));
        assert_eq!(pairs[1], ("f2".to_string(), "v2".to_string()));
    }

    #[test]
    fn test_tester_flat_pairs_odd_length() {
        // Odd-length array: last element is skipped (chunk len != 2)
        let v = redis::Value::Array(vec![
            redis::Value::BulkString(b"f1".to_vec()),
            redis::Value::BulkString(b"v1".to_vec()),
            redis::Value::BulkString(b"orphan".to_vec()),
        ]);
        let pairs = parse_flat_string_pairs(&v).unwrap();
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0], ("f1".to_string(), "v1".to_string()));
    }

    #[test]
    fn test_tester_flat_pairs_empty_array() {
        let v = redis::Value::Array(vec![]);
        let pairs = parse_flat_string_pairs(&v).unwrap();
        assert!(pairs.is_empty());
    }

    #[test]
    fn test_tester_flat_pairs_non_array() {
        let v = redis::Value::SimpleString("err".into());
        assert!(parse_flat_string_pairs(&v).is_err());
    }

    // -- parse_flat_string_array --

    #[test]
    fn test_tester_flat_array_normal() {
        let v = redis::Value::Array(vec![
            redis::Value::BulkString(b"a".to_vec()),
            redis::Value::Int(1),
            redis::Value::Nil,
        ]);
        let arr = parse_flat_string_array(&v).unwrap();
        assert_eq!(arr, vec!["a".to_string(), "1".to_string(), "".to_string()]);
    }

    #[test]
    fn test_tester_flat_array_non_array() {
        let v = redis::Value::Okay;
        assert!(parse_flat_string_array(&v).is_err());
    }

    // -- parse_hash_scan_result --

    #[test]
    fn test_tester_hash_scan_result_valid() {
        let v = redis::Value::Array(vec![
            redis::Value::Int(0),
            redis::Value::Array(vec![
                redis::Value::BulkString(b"field1".to_vec()),
                redis::Value::BulkString(b"val1".to_vec()),
            ]),
        ]);
        let (cursor, entries) = parse_hash_scan_result(&v).unwrap();
        assert_eq!(cursor, 0);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0], ("field1".to_string(), "val1".to_string()));
    }

    #[test]
    fn test_tester_hash_scan_result_wrong_length() {
        let v = redis::Value::Array(vec![redis::Value::Int(0)]);
        assert!(parse_hash_scan_result(&v).is_err());
    }

    #[test]
    fn test_tester_hash_scan_result_non_array() {
        let v = redis::Value::BulkString(b"err".to_vec());
        assert!(parse_hash_scan_result(&v).is_err());
    }

    // -- parse_scan_result_generic --

    #[test]
    fn test_tester_scan_generic_valid() {
        let v = redis::Value::Array(vec![
            redis::Value::Int(10),
            redis::Value::Array(vec![
                redis::Value::BulkString(b"m1".to_vec()),
                redis::Value::BulkString(b"m2".to_vec()),
            ]),
        ]);
        let (cursor, members) = parse_scan_result_generic(&v).unwrap();
        assert_eq!(cursor, 10);
        assert_eq!(members, vec!["m1".to_string(), "m2".to_string()]);
    }

    #[test]
    fn test_tester_scan_generic_cursor_zero() {
        let v = redis::Value::Array(vec![redis::Value::Int(0), redis::Value::Array(vec![])]);
        let (cursor, members) = parse_scan_result_generic(&v).unwrap();
        assert_eq!(cursor, 0);
        assert!(members.is_empty());
    }

    #[test]
    fn test_tester_scan_generic_non_array() {
        let v = redis::Value::Int(5);
        assert!(parse_scan_result_generic(&v).is_err());
    }

    // -- parse_zscan_result --

    #[test]
    fn test_tester_zscan_result_valid() {
        let v = redis::Value::Array(vec![
            redis::Value::Int(0),
            redis::Value::Array(vec![
                redis::Value::BulkString(b"member1".to_vec()),
                redis::Value::BulkString(b"1.5".to_vec()),
                redis::Value::BulkString(b"member2".to_vec()),
                redis::Value::BulkString(b"2.0".to_vec()),
            ]),
        ]);
        let (cursor, members) = parse_zscan_result(&v).unwrap();
        assert_eq!(cursor, 0);
        assert_eq!(members.len(), 2);
        assert_eq!(members[0].0, "member1");
        assert!((members[0].1 - 1.5).abs() < f64::EPSILON);
        assert_eq!(members[1].0, "member2");
        assert!((members[1].1 - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_tester_zscan_result_non_numeric_score() {
        let v = redis::Value::Array(vec![
            redis::Value::Int(0),
            redis::Value::Array(vec![
                redis::Value::BulkString(b"m".to_vec()),
                redis::Value::BulkString(b"not_a_number".to_vec()),
            ]),
        ]);
        let (_, members) = parse_zscan_result(&v).unwrap();
        assert_eq!(members.len(), 1);
        // unwrap_or(0.0) fallback
        assert!((members[0].1 - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_tester_zscan_result_odd_length_data() {
        let v = redis::Value::Array(vec![
            redis::Value::Int(0),
            redis::Value::Array(vec![
                redis::Value::BulkString(b"m".to_vec()),
                redis::Value::BulkString(b"1.0".to_vec()),
                redis::Value::BulkString(b"orphan".to_vec()),
            ]),
        ]);
        let (_, members) = parse_zscan_result(&v).unwrap();
        assert_eq!(members.len(), 1); // orphan skipped
    }

    #[test]
    fn test_tester_zscan_result_wrong_length() {
        let v = redis::Value::Array(vec![redis::Value::Int(0)]);
        assert!(parse_zscan_result(&v).is_err());
    }

    #[test]
    fn test_tester_zscan_result_non_array() {
        let v = redis::Value::BulkString(b"err".to_vec());
        assert!(parse_zscan_result(&v).is_err());
    }

    // -- parse_string_array --

    #[test]
    fn test_tester_string_array_valid() {
        let v = redis::Value::Array(vec![
            redis::Value::BulkString(b"item1".to_vec()),
            redis::Value::Int(42),
            redis::Value::Nil,
        ]);
        let arr = parse_string_array(&v).unwrap();
        assert_eq!(
            arr,
            vec!["item1".to_string(), "42".to_string(), "".to_string()]
        );
    }

    #[test]
    fn test_tester_string_array_empty() {
        let v = redis::Value::Array(vec![]);
        let arr = parse_string_array(&v).unwrap();
        assert!(arr.is_empty());
    }

    #[test]
    fn test_tester_string_array_non_array() {
        let v = redis::Value::Int(0);
        assert!(parse_string_array(&v).is_err());
    }

    // -- value_to_string edge cases --

    #[test]
    fn test_tester_value_to_string_bulk_string_utf8_lossy() {
        // Invalid UTF-8 bytes => lossy conversion
        let v = redis::Value::BulkString(vec![0xFF, 0xFE]);
        let s = value_to_string(&v);
        assert!(!s.is_empty());
    }
}
