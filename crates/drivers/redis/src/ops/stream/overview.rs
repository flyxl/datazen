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
    let mut filter_on_server = true;
    let truncated = 'scan: loop {
        let mut scan = redis::cmd("SCAN");
        scan.arg(cursor).arg("COUNT").arg(200);
        if filter_on_server {
            scan.arg("TYPE").arg("stream");
        }
        let raw: redis::Value = match scan.query_async(conn).await {
            Ok(raw) => raw,
            Err(error)
                if filter_on_server
                    && error.kind() == redis::ErrorKind::ResponseError
                    && error
                        .detail()
                        .is_some_and(|detail| detail.eq_ignore_ascii_case("syntax error")) =>
            {
                // Redis 5 does not understand SCAN TYPE. Retry the same page
                // once without the option; do not downgrade an ACL/IO failure.
                filter_on_server = false;
                continue;
            }
            Err(error) => return Err(error.to_string()),
        };
        let (next, batch) = parse_scan_result(&raw);
        let candidates = if filter_on_server || batch.is_empty() {
            batch
        } else {
            let mut pipe = redis::pipe();
            for key in &batch {
                pipe.cmd("TYPE").arg(key);
            }
            let types: Vec<redis::Value> =
                pipe.query_async(conn).await.map_err(|e| e.to_string())?;
            batch
                .into_iter()
                .zip(types)
                .filter_map(|(key, ty)| {
                    value_to_string(&ty)
                        .eq_ignore_ascii_case("stream")
                        .then_some(key)
                })
                .collect()
        };
        for key in candidates {
            if seen.insert(key.clone()) {
                // Prove another stream exists before declaring truncation.
                // SCAN can report duplicate keys while the data changes.
                if stream_keys.len() == limit {
                    break 'scan true;
                }
                stream_keys.push(key);
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
