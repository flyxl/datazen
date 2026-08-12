//! DUMP / RESTORE import-export helpers and manifest v1 types.

use std::collections::HashSet;

use redis::AsyncCommands;
use serde::{Deserialize, Serialize};

use crate::ops::KeyError;

#[allow(dead_code)] // manifest helpers reserved for the upcoming dump/restore import flow
pub const MANIFEST_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct DumpManifestKey {
    pub key: String,
    pub ttl_seconds: i64,
    pub dump_file: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct DumpManifest {
    pub schema_version: u32,
    pub db_index: u32,
    pub keys: Vec<DumpManifestKey>,
}

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

/// Build a safe, unique `.bin` filename for a Redis key inside the export zip.
#[allow(dead_code)]
pub fn dump_file_name_for_key(key: &str, index: usize, used: &mut HashSet<String>) -> String {
    let mut base: String = key
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if base.is_empty() || base.starts_with('.') {
        base = format!("key_{index}");
    }
    if base.len() > 120 {
        base.truncate(120);
    }
    let mut candidate = format!("{base}.bin");
    if !used.contains(&candidate) {
        used.insert(candidate.clone());
        return candidate;
    }
    let mut n = 2;
    loop {
        candidate = format!("{base}_{n}.bin");
        if !used.contains(&candidate) {
            used.insert(candidate.clone());
            return candidate;
        }
        n += 1;
    }
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

#[allow(dead_code)]
pub fn parse_manifest(json: &str) -> Result<DumpManifest, String> {
    let manifest: DumpManifest =
        serde_json::from_str(json).map_err(|e| format!("invalid manifest JSON: {e}"))?;
    if manifest.schema_version != MANIFEST_SCHEMA_VERSION {
        return Err(format!(
            "unsupported manifest schemaVersion: {} (expected {MANIFEST_SCHEMA_VERSION})",
            manifest.schema_version
        ));
    }
    Ok(manifest)
}

#[allow(dead_code)]
pub fn serialize_manifest(manifest: &DumpManifest) -> Result<String, String> {
    serde_json::to_string_pretty(manifest).map_err(|e| e.to_string())
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
    fn manifest_v1_serde_round_trip() {
        let manifest = DumpManifest {
            schema_version: MANIFEST_SCHEMA_VERSION,
            db_index: 0,
            keys: vec![
                DumpManifestKey {
                    key: "a".into(),
                    ttl_seconds: -1,
                    dump_file: "a.bin".into(),
                },
                DumpManifestKey {
                    key: "user:1".into(),
                    ttl_seconds: 3600,
                    dump_file: "user_1.bin".into(),
                },
            ],
        };
        let json = serialize_manifest(&manifest).unwrap();
        let parsed = parse_manifest(&json).unwrap();
        assert_eq!(parsed, manifest);
        assert!(json.contains("\"schemaVersion\": 1"));
        assert!(json.contains("\"dbIndex\": 0"));
    }

    #[test]
    fn parse_manifest_rejects_unsupported_schema() {
        let json = r#"{"schemaVersion":2,"dbIndex":0,"keys":[]}"#;
        let err = parse_manifest(json).unwrap_err();
        assert!(err.contains("unsupported manifest schemaVersion"));
    }

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
    fn dump_file_name_sanitizes_and_deduplicates() {
        let mut used = HashSet::new();
        assert_eq!(
            dump_file_name_for_key("a", 0, &mut used),
            "a.bin".to_string()
        );
        assert_eq!(
            dump_file_name_for_key("a", 1, &mut used),
            "a_2.bin".to_string()
        );
        let weird = dump_file_name_for_key("foo/bar:baz", 2, &mut used);
        assert!(weird.ends_with(".bin"));
        assert!(!weird.contains('/'));
    }

    #[test]
    fn dump_base64_round_trip() {
        let bytes = b"\x00\x01redis dump\xff";
        let encoded = encode_dump_base64(bytes);
        assert_eq!(decode_dump_base64(&encoded).unwrap(), bytes);
    }
}
