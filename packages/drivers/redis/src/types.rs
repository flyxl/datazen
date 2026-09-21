//! Redis-specific value/detail types crossing the driver↔host IPC boundary.
//!
//! Kept in the redis crate (not `driver-api`) because no `KeyValueDriver`
//! trait method references them: `get_key`/`get_key_raw` are Redis-inherent
//! commands dispatched via `execute_command`, not generic KV-trait calls.

use serde::{Deserialize, Serialize};

/// Parsed value detail returned by the `get_key` command.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyDetail {
    pub key: String,
    pub key_type: String,
    pub ttl: i64,
    pub value: serde_json::Value,
}

/// Binary-safe key value frame for the raw value channel.
///
/// Used by `get_key_raw` to transport the raw bytes of a string key as
/// base64, along with metadata for the detail header (size badge, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValueFrame {
    pub key: String,
    pub key_type: String,
    /// TTL in seconds: -1 = no expiry, -2 = key missing, >= 0 = remaining.
    pub ttl: i64,
    /// Logical length (STRLEN / HLEN / LLEN / SCARD / ZCARD / XLEN).
    pub logical_len: u64,
    /// MEMORY USAGE bytes when available (None if Redis < 4.0 or unsupported).
    pub mem_bytes: Option<u64>,
    /// Base64-encoded raw bytes of the string value. None when truncated or
    /// the key is not a string type.
    pub raw_b64: Option<String>,
    /// True when the value exceeded the size budget and was not fetched.
    pub truncated: bool,
}
