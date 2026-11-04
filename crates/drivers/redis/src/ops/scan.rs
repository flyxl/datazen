//! The shared `SCAN` walk (`scan_batch`) and the two keyspace readers built on it.

use crate::driver::parse_scan_result;
use redis::AsyncCommands;

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
