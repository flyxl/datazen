//! Shared value types and the pure helpers that resolve them: `TtlCommand` with its two TTL resolvers, the rename planner, and the batch reply payloads.

use serde::Serialize;

/// TTL resolution for relative seconds, absolute unix expiry, or persist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TtlCommand {
    /// Remove expiry (PERSIST).
    Persist,
    /// Relative expiry via EXPIRE (seconds from now).
    Expire(u64),
    /// Absolute expiry via EXPIREAT (unix timestamp seconds).
    ExpireAt(i64),
}

pub fn resolve_ttl(ttl_seconds: i64) -> Result<TtlCommand, String> {
    match ttl_seconds {
        -1 => Ok(TtlCommand::Persist),
        n if n >= 0 => Ok(TtlCommand::Expire(n as u64)),
        _ => Err(format!("invalid ttl_seconds: {ttl_seconds}")),
    }
}

/// Resolve absolute expire-at unix timestamp (must be > 0).
pub fn resolve_expire_at(expire_at: i64) -> Result<TtlCommand, String> {
    if expire_at <= 0 {
        return Err(format!(
            "invalid expire_at: {expire_at} (expected unix timestamp > 0)"
        ));
    }
    Ok(TtlCommand::ExpireAt(expire_at))
}

/// Build `(old_key, new_key)` pairs for keys that start with `old_prefix`.
pub fn plan_rename_prefix(
    old_prefix: &str,
    new_prefix: &str,
    keys: &[String],
) -> Vec<(String, String)> {
    keys.iter()
        .filter(|k| k.starts_with(old_prefix))
        .map(|k| {
            let suffix = &k[old_prefix.len()..];
            (k.clone(), format!("{new_prefix}{suffix}"))
        })
        .collect()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyError {
    pub key: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchDeleteResult {
    pub deleted: u64,
    pub errors: Vec<KeyError>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchSetTtlResult {
    pub updated: u64,
    pub errors: Vec<KeyError>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchRenameResult {
    pub renamed: u64,
    pub errors: Vec<KeyError>,
}
