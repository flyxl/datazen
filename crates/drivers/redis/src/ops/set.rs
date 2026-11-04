//! Set commands: `SADD` / `SREM` / `SSCAN`.

use super::parse::parse_scan_result_generic;
use redis::AsyncCommands;

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
