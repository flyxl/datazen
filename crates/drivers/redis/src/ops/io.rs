//! DUMP / RESTORE import-export helpers.

use redis::AsyncCommands;
use serde::{Deserialize, Serialize};

use crate::ops::KeyError;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DumpKeyEntry {
    pub key: String,
    pub ttl_seconds: i64,
    pub dump_base64: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DumpKeysResult {
    pub entries: Vec<DumpKeyEntry>,
    pub errors: Vec<KeyError>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreKeyEntry {
    pub key: String,
    pub ttl_seconds: i64,
    pub dump_base64: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreKeysResult {
    pub restored: u64,
    pub errors: Vec<KeyError>,
}

/// Convert PTTL (milliseconds) to manifest `ttlSeconds` (`-1` = no expiry).
pub fn pttl_to_ttl_seconds(pttl: i64) -> i64 {
    match pttl {
        -1 => -1,
        -2 => -2,
        ms if ms >= 0 => (ms + 999) / 1000,
        _ => -1,
    }
}

/// Convert manifest `ttlSeconds` to RESTORE TTL argument (milliseconds; `0` = persist).
pub fn ttl_seconds_to_restore_ms(ttl_seconds: i64) -> u64 {
    if ttl_seconds < 0 {
        0
    } else {
        ttl_seconds as u64 * 1000
    }
}

/// Whether RESTORE should include the `REPLACE` modifier.
pub fn restore_uses_replace(replace: bool) -> bool {
    replace
}

pub fn encode_dump_base64(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

pub fn decode_dump_base64(encoded: &str) -> Result<Vec<u8>, String> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(encoded.trim())
        .map_err(|e| format!("invalid dump base64: {e}"))
}

pub async fn dump_keys<C>(conn: &mut C, keys: &[String]) -> Result<DumpKeysResult, String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    let mut entries = Vec::new();
    let mut errors = Vec::new();

    for key in keys {
        match dump_single_key(conn, key).await {
            Ok(entry) => entries.push(entry),
            Err(error) => errors.push(KeyError {
                key: key.clone(),
                error,
            }),
        }
    }

    Ok(DumpKeysResult { entries, errors })
}

async fn dump_single_key<C>(conn: &mut C, key: &str) -> Result<DumpKeyEntry, String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    let dump: Vec<u8> = redis::cmd("DUMP")
        .arg(key)
        .query_async(conn)
        .await
        .map_err(|e| e.to_string())?;
    if dump.is_empty() {
        return Err("DUMP returned empty payload (key missing or unsupported type)".into());
    }

    let pttl: i64 = redis::cmd("PTTL")
        .arg(key)
        .query_async(conn)
        .await
        .map_err(|e| e.to_string())?;
    let ttl_seconds = pttl_to_ttl_seconds(pttl);

    Ok(DumpKeyEntry {
        key: key.to_string(),
        ttl_seconds,
        dump_base64: encode_dump_base64(&dump),
    })
}

pub async fn restore_keys<C>(
    conn: &mut C,
    entries: &[RestoreKeyEntry],
    replace: bool,
) -> Result<RestoreKeysResult, String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    let mut restored = 0u64;
    let mut errors = Vec::new();

    for entry in entries {
        match restore_single_key(conn, entry, replace).await {
            Ok(()) => restored += 1,
            Err(error) => errors.push(KeyError {
                key: entry.key.clone(),
                error,
            }),
        }
    }

    Ok(RestoreKeysResult { restored, errors })
}

async fn restore_single_key<C>(
    conn: &mut C,
    entry: &RestoreKeyEntry,
    replace: bool,
) -> Result<(), String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    let dump = decode_dump_base64(&entry.dump_base64)?;
    if dump.is_empty() {
        return Err("dump payload is empty".into());
    }

    let ttl_ms = ttl_seconds_to_restore_ms(entry.ttl_seconds);
    let mut cmd = redis::cmd("RESTORE");
    cmd.arg(&entry.key).arg(ttl_ms).arg(dump);
    if restore_uses_replace(replace) {
        cmd.arg("REPLACE");
    }
    cmd.query_async::<()>(conn).await.map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pttl_to_ttl_seconds_mapping() {
        assert_eq!(pttl_to_ttl_seconds(-1), -1);
        assert_eq!(pttl_to_ttl_seconds(-2), -2);
        assert_eq!(pttl_to_ttl_seconds(0), 0);
        assert_eq!(pttl_to_ttl_seconds(1500), 2);
        assert_eq!(pttl_to_ttl_seconds(3600_000), 3600);
    }

    #[test]
    fn ttl_seconds_to_restore_ms_mapping() {
        assert_eq!(ttl_seconds_to_restore_ms(-1), 0);
        assert_eq!(ttl_seconds_to_restore_ms(0), 0);
        assert_eq!(ttl_seconds_to_restore_ms(3600), 3_600_000);
    }

    #[test]
    fn restore_replace_flag_controls_modifier() {
        assert!(!restore_uses_replace(false));
        assert!(restore_uses_replace(true));
    }

    #[test]
    fn dump_base64_round_trip() {
        let bytes = b"\x00\x01redis dump\xff";
        let encoded = encode_dump_base64(bytes);
        assert_eq!(decode_dump_base64(&encoded).unwrap(), bytes);
    }
}
