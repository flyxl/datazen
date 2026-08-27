//! Per-extension key-value storage backed by `{plugins_dir}/{id}/.storage.json`.
//!
//! Values are namespaced by extension directory, so two extensions writing the same
//! key can never interfere. Writes are atomic (temp file + rename) and capped
//! at 1 MB per extension.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use serde_json::{Map, Value};

/// Host-managed per-extension KV file (hidden from package scans and reads).
pub const STORAGE_FILE: &str = ".storage.json";

/// Maximum serialized storage size per extension.
pub const MAX_STORAGE_BYTES: usize = 1024 * 1024;

/// Serializes read-modify-write cycles so concurrent updates cannot clobber
/// each other's temp files or lose writes.
static STORAGE_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

/// Validate an extension id used as a path segment for storage operations.
fn validate_extension_id(extension_id: &str) -> Result<(), String> {
    if extension_id.is_empty()
        || extension_id.starts_with('.')
        || extension_id.contains('/')
        || extension_id.contains('\\')
        || extension_id.contains("..")
        || extension_id.contains('\0')
    {
        return Err(format!("invalid extension id: {extension_id}"));
    }
    Ok(())
}

fn validate_key(key: &str) -> Result<(), String> {
    if key.is_empty() || key.contains('\0') {
        return Err("storage key must not be empty".into());
    }
    Ok(())
}

pub(crate) fn storage_file_path(plugins_dir: &Path, extension_id: &str) -> Result<PathBuf, String> {
    validate_extension_id(extension_id)?;
    Ok(plugins_dir.join(extension_id).join(STORAGE_FILE))
}

fn read_storage_map(path: &Path) -> Result<Map<String, Value>, String> {
    if !path.is_file() {
        return Ok(Map::new());
    }
    let content = fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let value: Value =
        serde_json::from_str(&content).map_err(|e| format!("parse {}: {e}", path.display()))?;
    match value {
        Value::Object(map) => Ok(map),
        _ => Err(format!("{} must contain a JSON object", path.display())),
    }
}

fn write_storage_atomic(path: &Path, map: &Map<String, Value>) -> Result<(), String> {
    let serialized =
        serde_json::to_string_pretty(map).map_err(|e| format!("encode storage: {e}"))?;
    if serialized.len() > MAX_STORAGE_BYTES {
        return Err(format!(
            "extension storage exceeds limit ({MAX_STORAGE_BYTES} bytes)"
        ));
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create storage dir: {e}"))?;
    }

    // Temp file lives next to the target so rename stays on one filesystem.
    let tmp_path = path.with_file_name(format!(
        "{STORAGE_FILE}.tmp-{}",
        uuid::Uuid::new_v4().simple()
    ));

    let write_result = (|| -> std::io::Result<()> {
        fs::write(&tmp_path, serialized.as_bytes())?;
        fs::rename(&tmp_path, path)
    })();

    if let Err(e) = write_result {
        let _ = fs::remove_file(&tmp_path);
        return Err(format!("write {}: {e}", path.display()));
    }
    Ok(())
}

/// Read a single key; `None` when the extension has no stored value for it.
pub fn storage_get(
    plugins_dir: &Path,
    extension_id: &str,
    key: &str,
) -> Result<Option<Value>, String> {
    validate_key(key)?;
    let path = storage_file_path(plugins_dir, extension_id)?;

    let _guard = STORAGE_LOCK.lock().map_err(|_| "storage lock poisoned")?;
    read_storage_map(&path).map(|map| map.get(key).cloned())
}

/// Write a single key. Fails when the resulting serialized store would exceed
/// [`MAX_STORAGE_BYTES`].
pub fn storage_set(
    plugins_dir: &Path,
    extension_id: &str,
    key: &str,
    value: Value,
) -> Result<(), String> {
    validate_key(key)?;
    let path = storage_file_path(plugins_dir, extension_id)?;

    let _guard = STORAGE_LOCK.lock().map_err(|_| "storage lock poisoned")?;
    let mut map = read_storage_map(&path)?;
    map.insert(key.to_string(), value);
    write_storage_atomic(&path, &map)
}

/// Delete a single key; returns whether it existed.
pub fn storage_remove(plugins_dir: &Path, extension_id: &str, key: &str) -> Result<bool, String> {
    validate_key(key)?;
    let path = storage_file_path(plugins_dir, extension_id)?;

    let _guard = STORAGE_LOCK.lock().map_err(|_| "storage lock poisoned")?;
    let mut map = read_storage_map(&path)?;
    let removed = map.remove(key).is_some();
    if removed {
        if map.is_empty() {
            if path.is_file() && fs::remove_file(path).is_err() {
                return Err("remove storage file failed".into());
            }
        } else {
            write_storage_atomic(&path, &map)?;
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::sync::Barrier;
    use tempfile::TempDir;

    #[test]
    fn set_get_roundtrip() {
        let dir = TempDir::new().unwrap();
        storage_set(dir.path(), "acme.bill-audit", "lastUid", json!(58043285)).unwrap();
        assert_eq!(
            storage_get(dir.path(), "acme.bill-audit", "lastUid").unwrap(),
            Some(json!(58043285))
        );
        assert!(dir.path().join("acme.bill-audit/.storage.json").is_file());
    }

    #[test]
    fn plugins_are_isolated_by_namespace() {
        let dir = TempDir::new().unwrap();
        storage_set(dir.path(), "acme.one", "shared-key", json!("from-one")).unwrap();
        storage_set(dir.path(), "acme.two", "shared-key", json!(42)).unwrap();

        assert_eq!(
            storage_get(dir.path(), "acme.one", "shared-key").unwrap(),
            Some(json!("from-one"))
        );
        assert_eq!(
            storage_get(dir.path(), "acme.two", "shared-key").unwrap(),
            Some(json!(42))
        );

        storage_remove(dir.path(), "acme.one", "shared-key").unwrap();
        assert_eq!(
            storage_get(dir.path(), "acme.one", "shared-key").unwrap(),
            None
        );
        assert_eq!(
            storage_get(dir.path(), "acme.two", "shared-key").unwrap(),
            Some(json!(42))
        );
    }

    #[test]
    fn get_on_missing_plugin_or_key_is_none() {
        let dir = TempDir::new().unwrap();
        assert_eq!(storage_get(dir.path(), "acme.none", "k").unwrap(), None);
        storage_set(dir.path(), "acme.none", "other", json!(1)).unwrap();
        assert_eq!(storage_get(dir.path(), "acme.none", "k").unwrap(), None);
    }

    #[test]
    fn remove_reports_presence_and_cleans_file_when_empty() {
        let dir = TempDir::new().unwrap();
        assert!(!storage_remove(dir.path(), "acme.x", "gone").unwrap());

        storage_set(dir.path(), "acme.x", "keep", json!(1)).unwrap();
        storage_set(dir.path(), "acme.x", "gone", json!(2)).unwrap();
        assert!(storage_remove(dir.path(), "acme.x", "gone").unwrap());
        assert_eq!(storage_get(dir.path(), "acme.x", "gone").unwrap(), None);
        assert_eq!(
            storage_get(dir.path(), "acme.x", "keep").unwrap(),
            Some(json!(1))
        );

        assert!(storage_remove(dir.path(), "acme.x", "keep").unwrap());
        assert!(!dir.path().join("acme.x/.storage.json").exists());
    }

    #[test]
    fn rejects_oversized_store() {
        let dir = TempDir::new().unwrap();
        let big = "x".repeat(MAX_STORAGE_BYTES + 1);
        let err = storage_set(dir.path(), "acme.big", "blob", json!(big)).unwrap_err();
        assert!(err.contains("exceeds limit"), "unexpected: {err}");
        assert!(!dir.path().join("acme.big/.storage.json").exists());

        // Existing small values stay writable afterwards.
        storage_set(dir.path(), "acme.big", "small", json!("ok")).unwrap();
    }

    #[test]
    fn rejects_invalid_extension_ids_and_keys() {
        let dir = TempDir::new().unwrap();
        for bad in ["../escape", ".hidden", "a/b", "a\\b", ""] {
            assert!(
                storage_set(dir.path(), bad, "k", json!(1)).is_err(),
                "extension id `{bad}` should be rejected"
            );
        }
        assert!(storage_set(dir.path(), "acme.ok", "", json!(1)).is_err());
        assert_eq!(
            fs::read_dir(dir.path()).unwrap().count(),
            0,
            "nothing written"
        );
    }

    #[test]
    fn concurrent_atomic_writes_never_corrupt_store() {
        let dir = TempDir::new().unwrap();
        const WRITERS: usize = 8;
        const KEYS_PER_WRITER: usize = 25;

        let barrier = std::sync::Arc::new(Barrier::new(WRITERS));
        let mut handles = Vec::new();
        for w in 0..WRITERS {
            let barrier = barrier.clone();
            let root = dir.path().to_path_buf();
            handles.push(std::thread::spawn(move || {
                barrier.wait();
                for i in 0..KEYS_PER_WRITER {
                    let key = format!("w{w}-key{i}");
                    storage_set(&root, "acme.parallel", &key, json!(format!("v{w}-{i}"))).unwrap();
                }
            }));
        }
        for handle in handles {
            handle.join().unwrap();
        }

        let content = fs::read_to_string(dir.path().join("acme.parallel/.storage.json")).unwrap();
        let parsed: BTreeMap<String, Value> = serde_json::from_str(&content).expect("valid JSON");
        assert_eq!(parsed.len(), WRITERS * KEYS_PER_WRITER);

        for w in 0..WRITERS {
            for i in 0..KEYS_PER_WRITER {
                let key = format!("w{w}-key{i}");
                assert_eq!(
                    storage_get(dir.path(), "acme.parallel", &key).unwrap(),
                    Some(json!(format!("v{w}-{i}")))
                );
            }
        }
    }

    #[test]
    fn no_tmp_files_left_behind() {
        let dir = TempDir::new().unwrap();
        for i in 0..10 {
            storage_set(dir.path(), "acme.tmp-check", &format!("k{i}"), json!(i)).unwrap();
        }
        let leftovers: Vec<_> = fs::read_dir(dir.path().join("acme.tmp-check"))
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp-"))
            .collect();
        assert!(leftovers.is_empty(), "leftover tmp files: {leftovers:?}");
    }
}
