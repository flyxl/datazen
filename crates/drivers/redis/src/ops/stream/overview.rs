//! `stream_overview`: the bounded `SCAN` + per-key summary the stream panel renders.

use super::parse::parse_xinfo_groups;
use super::parse::value_to_string;
use super::types::StreamOverviewResult;
use super::types::StreamOverviewRow;
use crate::driver::parse_scan_result;
use redis::AsyncCommands;

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
