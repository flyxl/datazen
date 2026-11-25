//! Native Redis overview. INFO is the only mandatory server round trip.

use std::collections::BTreeMap;
use std::fmt::Write;

use datazen_driver_api::{ConnectionHandle, DatabaseDriver};
use serde_json::{json, Value};

use super::call;

const METRICS: &[(&str, &[(&str, &str)])] = &[
    (
        "Server",
        &[
            ("Redis version", "redis_version"),
            ("Mode", "redis_mode"),
            ("Role", "role"),
            ("Uptime (s)", "uptime_in_seconds"),
            ("Connected clients", "connected_clients"),
            ("Blocked clients", "blocked_clients"),
        ],
    ),
    (
        "Performance",
        &[
            ("Ops/s", "instantaneous_ops_per_sec"),
            ("Total commands", "total_commands_processed"),
            ("Used memory", "used_memory_human"),
            ("Max memory", "maxmemory_human"),
            ("Fragmentation ratio", "mem_fragmentation_ratio"),
            ("Evicted keys", "evicted_keys"),
            ("Expired keys", "expired_keys"),
        ],
    ),
];

fn info_summary(info: &str) -> String {
    let fields: BTreeMap<_, _> = info
        .lines()
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| line.split_once(':'))
        .collect();
    let mut summary = String::new();
    for (section, metrics) in METRICS {
        let _ = writeln!(summary, "{section}");
        for (label, key) in *metrics {
            if let Some(value) = fields.get(key) {
                let _ = writeln!(summary, "{label}: {value}");
            }
        }
        summary.push('\n');
    }
    let _ = writeln!(summary, "Keyspace");
    let mut databases: Vec<_> = fields
        .iter()
        .filter_map(|(db, value)| Some((db.strip_prefix("db")?.parse::<u32>().ok()?, value)))
        .collect();
    databases.sort_unstable_by_key(|(db, _)| *db);
    if databases.is_empty() {
        let _ = writeln!(summary, "No populated databases reported.");
    } else {
        for (db, value) in databases.into_iter().take(32) {
            let _ = writeln!(summary, "DB {db}: {value}");
        }
    }
    summary
}

pub(super) async fn overview(
    driver: &dyn DatabaseDriver,
    handle: &ConnectionHandle,
) -> Result<String, String> {
    let info = call(driver, handle, "info", json!({})).await?;
    let raw = info
        .as_str()
        .ok_or("Redis returned an invalid INFO response.")?;
    let mut summary = info_summary(raw);
    summary.push_str("\nRecent slow commands\n");
    match call(driver, handle, "slowlog_get", json!({"count":3})).await {
        Ok(rows) if rows.as_array().is_some_and(Vec::is_empty) => summary.push_str("No entries.\n"),
        Ok(rows) => {
            for row in rows.as_array().into_iter().flatten().take(3) {
                // Display the verb only: arguments may contain application data.
                let verb = row["command"]
                    .get(0)
                    .and_then(Value::as_str)
                    .unwrap_or("Unknown");
                let verb: String = verb.chars().filter(|c| !c.is_control()).take(32).collect();
                let latency = row["durationUs"].as_u64().unwrap_or(0);
                let _ = writeln!(summary, "{verb}: {latency} µs");
            }
        }
        Err(_) => summary.push_str("Unavailable with this connection's permissions.\n"),
    }
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info_summary_formats_sparse_keyspace_in_numeric_order() {
        let data = "# Server\r\nredis_version:8.0\r\n# Keyspace\r\ndb10:keys=1\r\ndb2:keys=3\r\n";
        let summary = info_summary(data);
        assert!(summary.contains("Redis version: 8.0"));
        assert!(summary.find("DB 2:").unwrap() < summary.find("DB 10:").unwrap());
        assert!(!summary.contains("Connected clients:"));
    }
}
