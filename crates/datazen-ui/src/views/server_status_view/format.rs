//! Pure ports of the formatting/label helpers in `ServerStatusView.tsx` and
//! `lib/serverStatusTrends.ts`. No host state, IPC or timer lives here.

use crate::i18n::t;

use super::{StatusValue, TrendPointVm};

/// React `serverStatus.autoRefresh` options `(ms, i18n key)` in order.
#[allow(dead_code)] // contract mirror; consumed by the refresh select and tests.
pub const AUTO_REFRESH_OPTIONS: [(u64, &str); 4] = [
    (0, "serverStatus.refreshOff"),
    (5000, "serverStatus.refresh5s"),
    (10000, "serverStatus.refresh10s"),
    (30000, "serverStatus.refresh30s"),
];

/// React `METRIC_LABELS`: status field → i18n key (only the mapped fields get a
/// localized label; the rest fall back to the raw key).
#[allow(dead_code)] // pure mapping; consumed by cards/details and tests.
pub fn metric_label_key(key: &str) -> Option<&'static str> {
    match key {
        "version" => Some("serverStatus.version"),
        "database" => Some("serverStatus.database"),
        "uptimeSeconds" => Some("serverStatus.uptime"),
        "connections" => Some("serverStatus.connections"),
        "maxConnections" => Some("serverStatus.maxConnections"),
        "activeQueries" => Some("serverStatus.activeQueries"),
        "databaseSize" => Some("serverStatus.databaseSize"),
        "qps" => Some("common.queriesPerSec"),
        "slowQueries" => Some("serverStatus.slowQueries"),
        "networkIn" => Some("serverStatus.networkIn"),
        "networkOut" => Some("serverStatus.networkOut"),
        "deadlocks" => Some("serverStatus.deadlocks"),
        "tempFiles" => Some("serverStatus.tempFiles"),
        "tempBytes" => Some("serverStatus.tempBytes"),
        "walBytes" => Some("serverStatus.walRate"),
        "cacheHitRatio" => Some("serverStatus.cacheHitRatio"),
        "innodbHitRatio" => Some("serverStatus.innodbHitRatio"),
        "txsPerSec" => Some("common.queriesPerSec"),
        _ => None,
    }
}

/// React `labelForKey`: the localized label or the raw key.
#[allow(dead_code)] // pure helper; consumed by details and tests.
pub fn label_for_key(key: &str) -> String {
    metric_label_key(key).map(t).unwrap_or(key).to_owned()
}

/// React `numberFrom`: a number, or a finite numeric string, else null.
#[allow(dead_code)] // pure helper; consumed by cards and tests.
pub fn number_from(value: &StatusValue) -> Option<f64> {
    match value {
        StatusValue::Num(n) => Some(*n),
        StatusValue::Str(s) if !s.trim().is_empty() => {
            let parsed = s.trim().parse::<f64>().ok()?;
            parsed.is_finite().then_some(parsed)
        }
        _ => None,
    }
}

/// React `formatUptime`: the `{days}` or `{hours}/{minutes}/{seconds}` form.
#[allow(dead_code)] // pure helper; consumed by cards/details and tests.
pub fn format_uptime(seconds: f64) -> String {
    let total = seconds.max(0.0) as i64;
    let days = total / 86400;
    let hours = (total % 86400) / 3600;
    let minutes = (total % 3600) / 60;
    if days > 0 {
        t("serverStatus.uptimeFormatDays")
            .replace("{days}", &days.to_string())
            .replace("{hours}", &hours.to_string())
            .replace("{minutes}", &minutes.to_string())
    } else {
        t("serverStatus.uptimeFormat")
            .replace("{hours}", &hours.to_string())
            .replace("{minutes}", &minutes.to_string())
            .replace("{seconds}", &seconds.to_string())
    }
}

/// React `formatValue`: null/empty → em dash, uptime → formatted, else string.
#[allow(dead_code)] // pure helper; consumed by details and tests.
pub fn format_value(key: &str, value: &StatusValue) -> String {
    if value.is_absent() {
        return "—".to_owned();
    }
    if key == "uptimeSeconds" {
        // React formats uptime only for `typeof value === 'number'`; a numeric
        // string such as "100" is returned unchanged.
        if let StatusValue::Num(seconds) = value {
            return format_uptime(*seconds);
        }
    }
    value.display()
}

/// React `formatClock(ts)`: `HH:mm:ss` from wall-clock ms (UTC, the port's
/// deterministic stand-in for the browser's local timezone).
#[allow(dead_code)] // pure helper; consumed by the header and tests.
pub fn format_clock(ts: i64) -> String {
    let secs = ts.max(0) / 1000;
    let hour = (secs / 3600) % 24;
    let minute = (secs / 60) % 60;
    let second = secs % 60;
    format!("{hour:02}:{minute:02}:{second:02}")
}

/// React `latestSeriesValue`: last point of a series, or null.
#[allow(dead_code)] // pure helper; consumed by cards and tests.
pub fn latest_series_value(series: Option<&[TrendPointVm]>) -> Option<f64> {
    series.and_then(|points| points.last()).map(|point| point.v)
}

/// React `formatRate`: compact per-second number.
#[allow(dead_code)] // pure helper; consumed by cards and tests.
pub fn format_rate(value: f64) -> String {
    if value < 10.0 {
        format!("{value:.2}")
    } else if value < 1000.0 {
        format!("{value:.1}")
    } else if value < 1_000_000.0 {
        format!("{:.1}K", value / 1000.0)
    } else {
        format!("{:.1}M", value / 1_000_000.0)
    }
}

/// React `cardRateFromHistory`: latest trend point formatted, else the fallback.
#[allow(dead_code)] // pure helper; consumed by cards and tests.
pub fn card_rate_from_history(
    history: &std::collections::BTreeMap<String, Vec<TrendPointVm>>,
    chart_key: &str,
    fallback: &StatusValue,
) -> String {
    if let Some(rate) = latest_series_value(history.get(chart_key).map(Vec::as_slice)) {
        return format_rate(rate);
    }
    if fallback.is_absent() {
        "—".to_owned()
    } else {
        fallback.display()
    }
}

/// React `formatBytes`: bytes with a binary unit.
#[allow(dead_code)] // pure helper; consumed by cards and tests.
pub fn format_bytes(value: f64) -> String {
    if value < 1024.0 {
        format!("{value:.1} B")
    } else if value < 1024.0 * 1024.0 {
        format!("{:.1} KB", value / 1024.0)
    } else if value < 1024.0 * 1024.0 * 1024.0 {
        format!("{:.1} MB", value / 1024.0 / 1024.0)
    } else {
        format!("{:.1} GB", value / 1024.0 / 1024.0 / 1024.0)
    }
}

/// React `formatByteRate`: bytes with a `/s` suffix.
#[allow(dead_code)] // pure helper; consumed by cards and tests.
pub fn format_byte_rate(value: f64) -> String {
    format!("{}/s", format_bytes(value))
}

/// React `detailRows`' internal keys: cumulative counters and trend-only series
/// are hidden from the details table (they belong to cards/charts).
#[allow(dead_code)] // pure rule; consumed by details and tests.
pub fn is_internal_detail_key(key: &str) -> bool {
    matches!(
        key,
        "questionsCounter"
            | "newSessionsCounter"
            | "bytesInCounter"
            | "bytesOutCounter"
            | "xactCommitCounter"
            | "xactRollbackCounter"
            | "blksReadCounter"
            | "blksHitCounter"
            | "tupFetchedCounter"
            | "tupReturnedCounter"
            | "tupInsertedCounter"
            | "tupUpdatedCounter"
            | "tupDeletedCounter"
            | "tempBytesCounter"
            | "walBytes"
            | "tempBytes"
            | "sessionTotal"
            | "sessionActive"
            | "sessionIdle"
            | "statementCounters"
            | "statusVariables"
    )
}

/// React `detailRows`' preferred key order; unlisted keys follow sorted.
#[allow(dead_code)] // pure rule; consumed by details and tests.
pub const DETAIL_ORDER: [&str; 16] = [
    "version",
    "database",
    "uptimeSeconds",
    "connections",
    "maxConnections",
    "activeQueries",
    "databaseSize",
    "qps",
    "txsPerSec",
    "slowQueries",
    "networkIn",
    "networkOut",
    "cacheHitRatio",
    "innodbHitRatio",
    "deadlocks",
    "tempFiles",
];

/// React `detailRows` key order: ordered fields first, then the rest sorted.
#[allow(dead_code)] // pure rule; consumed by details and tests.
pub fn detail_keys(status: &std::collections::BTreeMap<String, StatusValue>) -> Vec<String> {
    let mut keys: Vec<String> = DETAIL_ORDER
        .iter()
        .filter(|key| status.contains_key(**key) && !is_internal_detail_key(key))
        .map(|key| (*key).to_owned())
        .collect();
    let mut rest: Vec<String> = status
        .keys()
        .filter(|key| !DETAIL_ORDER.contains(&key.as_str()) && !is_internal_detail_key(key))
        .cloned()
        .collect();
    rest.sort();
    keys.extend(rest);
    keys
}
