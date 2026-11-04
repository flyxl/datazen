//! The reply shapes: stream entries, group / consumer / pending payloads, and the overview row the UI sorts by.

use serde::Serialize;
use std::collections::HashMap;

/// Default number of stream keys to sample when `limit` is omitted or zero.
pub const DEFAULT_STREAM_OVERVIEW_LIMIT: u32 = 100;

/// Resolve the effective stream overview limit (defaults to [`DEFAULT_STREAM_OVERVIEW_LIMIT`]).
pub fn resolve_stream_overview_limit(limit: Option<u32>) -> u32 {
    match limit {
        Some(0) | None => DEFAULT_STREAM_OVERVIEW_LIMIT,
        Some(n) => n,
    }
}

/// Validate a consumer group name (non-empty after trim).
pub fn validate_xgroup_name(name: &str) -> Result<String, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("consumer group name is required".into());
    }
    Ok(trimmed.to_string())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamEntry {
    pub id: String,
    pub fields: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct XrangeResult {
    pub entries: Vec<StreamEntry>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct XaddResult {
    pub id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamGroupInfo {
    pub name: String,
    pub consumers: u64,
    pub pending: u64,
    pub last_delivered_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct XpendingEntry {
    pub id: String,
    pub consumer: String,
    pub idle_ms: u64,
    pub delivery_count: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct XpendingResult {
    pub total: u64,
    pub entries: Vec<XpendingEntry>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsumerInfo {
    pub name: String,
    pub pending: u64,
    pub idle_ms: u64,
    pub delivery_count: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamLagResult {
    pub lag: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamOverviewRow {
    pub key: String,
    pub length: u64,
    pub group_count: u64,
    pub pending_total: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamOverviewResult {
    pub rows: Vec<StreamOverviewRow>,
    pub truncated: bool,
}
