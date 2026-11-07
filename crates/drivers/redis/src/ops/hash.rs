//! Hash commands: `HSET` / `HDEL` / `HSCAN` and the `HSCAN` reply parser.

use super::parse::parse_cursor_from_value;
use super::parse::parse_flat_string_pairs;
use redis::AsyncCommands;

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

pub(crate) fn parse_hash_scan_result(
    raw: &redis::Value,
) -> Result<(u64, Vec<(String, String)>), String> {
    match raw {
        redis::Value::Array(items) if items.len() == 2 => {
            let next_cursor = parse_cursor_from_value(&items[0])?;
            let pairs = parse_flat_string_pairs(&items[1])?;
            Ok((next_cursor, pairs))
        }
        _ => Err(format!("unexpected HSCAN response: {raw:?}")),
    }
}
