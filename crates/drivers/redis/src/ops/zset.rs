//! Sorted-set commands: `ZADD` / `ZREM` / `ZSCAN` and `ZsetMember`.

use super::parse::parse_zscan_result;
use redis::AsyncCommands;

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
