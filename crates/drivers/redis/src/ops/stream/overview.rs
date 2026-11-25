//! `stream_overview`: the bounded `SCAN` + per-key summary the stream panel renders.

use super::parse::parse_xinfo_groups;
use super::parse::value_to_string;
use super::types::StreamOverviewResult;
use super::types::StreamOverviewRow;
use crate::driver::parse_scan_result;
use redis::AsyncCommands;
use std::collections::HashSet;

pub async fn stream_overview<C>(conn: &mut C, limit: u32) -> Result<StreamOverviewResult, String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    let limit = limit.max(1) as usize;
    let mut stream_keys = Vec::new();
    let mut seen = HashSet::new();
    let mut cursor = 0u64;
    let truncated = 'scan: loop {
        let raw: redis::Value = redis::cmd("SCAN")
            .arg(cursor)
            .arg("COUNT")
            .arg(200)
            .query_async(conn)
            .await
            .map_err(|e| e.to_string())?;
        let (next, batch) = parse_scan_result(&raw);
        if !batch.is_empty() {
            let mut pipe = redis::pipe();
            for key in &batch {
                pipe.cmd("TYPE").arg(key);
            }
            let types: Vec<redis::Value> =
                pipe.query_async(conn).await.map_err(|e| e.to_string())?;

            for (key, ty) in batch.into_iter().zip(types.into_iter()) {
                if value_to_string(&ty).eq_ignore_ascii_case("stream") && seen.insert(key.clone()) {
                    // Prove another stream exists before declaring truncation.
                    // Total database size includes unrelated key types and SCAN
                    // can report duplicate keys while the data changes.
                    if stream_keys.len() == limit {
                        break 'scan true;
                    }
                    stream_keys.push(key);
                }
            }
        }
        if next == 0 {
            break 'scan false;
        }
        cursor = next;
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

    Ok(StreamOverviewResult { rows, truncated })
}
