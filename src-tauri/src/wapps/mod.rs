//! Runtime UI/theme extension base: registry over installed wapp packages.
//!
//! Wapps live in `{appData}/wapps/{id}/` (folder name == `manifest.id`).
//! [`WappManager`] loads every valid package at startup, tracks enabled
//! state via a `.enabled` marker file, and is shared through `AppState`.
pub mod install;
pub mod manifest;
pub mod protocol;
pub mod storage;

#[cfg(test)]
mod integration_tests;

// F9: keeps the E2E sample wapp package (e2e/fixtures/sample-wapp) valid.
#[cfg(test)]
mod fixture_tests;

#[cfg(test)]
mod protocol_security_tests;

#[cfg(test)]
mod manifest_tests;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

#[allow(unused_imports)]
pub use manifest::{
    allowed_wapp_file_ext, is_valid_wapp_id, parse_manifest, validate_manifest, validate_wapp_dir,
    Contributions, PageContribution, Permission, ThemeContribution, WappManifest, MAX_WAPP_FILES,
    MAX_WAPP_UNCOMPRESSED,
};
#[allow(unused_imports)]
pub use protocol::{handle_datazen_request, parse_datazen_uri, WAPPS_OPEN_PAGE_EVENT};
pub use storage::{storage_get, storage_remove, storage_set};

/// Host-side runtime wapp API version; packages must declare
/// `apiVersion == WAPP_API_VERSION` to load.
pub const WAPP_API_VERSION: u32 = 2;

/// Marker file inside a wapp directory; presence means "enabled".
pub const ENABLED_MARKER_FILE: &str = ".enabled";

#[derive(Debug, Clone)]
pub struct LoadedWapp {
    pub manifest: WappManifest,
    pub enabled: bool,
}

/// Shared wapp registry. All methods are thread-safe; disk writes are small
/// marker-file operations.
#[derive(Debug)]
pub struct WappManager {
    wapps_dir: PathBuf,
    wapps: RwLock<HashMap<String, LoadedWapp>>,
}

impl WappManager {
    pub fn new(wapps_dir: PathBuf) -> Self {
        Self {
            wapps_dir,
            wapps: RwLock::new(HashMap::new()),
        }
    }

    /// Root directory that holds one sub-directory per installed wapp.
    pub fn wapps_dir(&self) -> &Path {
        &self.wapps_dir
    }

    /// Directory of an installed wapp (id must be validated first).
    pub fn wapp_dir(&self, id: &str) -> PathBuf {
        self.wapps_dir.join(id)
    }

    fn checked_wapp_dir(&self, id: &str) -> Result<PathBuf, String> {
        if !is_valid_wapp_id(id) {
            return Err(format!("invalid wapp id: {id}"));
        }
        Ok(self.wapp_dir(id))
    }

    /// Scan `{wapps_dir}` and register every valid package. Invalid or
    /// foreign directories are skipped with a warning. Staging/backup entries
    /// (dot-prefixed) are ignored. Returns the number of loaded wapps.
    pub fn load_from_disk(&self) -> usize {
        let mut map = self.wapps.write().unwrap_or_else(|e| {
            tracing::warn!(error = %e, "wapp registry write lock poisoned; recovering");
            e.into_inner()
        });
        map.clear();

        let Ok(entries) = fs::read_dir(&self.wapps_dir) else {
            return 0;
        };

        let mut loaded = 0usize;
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if name.starts_with('.') || !path.is_dir() {
                continue;
            }

            match validate_wapp_dir(&path) {
                Ok(manifest) => {
                    let enabled = self
                        .wapp_dir(&manifest.id)
                        .join(ENABLED_MARKER_FILE)
                        .exists();
                    tracing::debug!(wapp = %manifest.id, enabled, "loaded ui wapp");
                    map.insert(manifest.id.clone(), LoadedWapp { manifest, enabled });
                    loaded += 1;
                }
                Err(e) => {
                    tracing::warn!(wapp = %name, error = %e, "skipping invalid ui wapp");
                }
            }
        }

        loaded
    }

    /// Snapshot of all registered wapps, sorted by name then id.
    pub fn list(&self) -> Vec<LoadedWapp> {
        let map = self.wapps.read().unwrap_or_else(|e| {
            tracing::warn!(error = %e, "wapp registry read lock poisoned; recovering");
            e.into_inner()
        });
        let mut wapps: Vec<LoadedWapp> = map.values().cloned().collect();
        wapps.sort_by(|a, b| {
            a.manifest
                .name
                .cmp(&b.manifest.name)
                .then_with(|| a.manifest.id.cmp(&b.manifest.id))
        });
        wapps
    }

    pub fn get(&self, id: &str) -> Option<LoadedWapp> {
        self.wapps
            .read()
            .unwrap_or_else(|e| {
                tracing::warn!(error = %e, "wapp registry read lock poisoned; recovering");
                e.into_inner()
            })
            .get(id)
            .cloned()
    }

    pub fn manifest(&self, id: &str) -> Option<WappManifest> {
        self.get(id).map(|p| p.manifest)
    }

    pub fn is_enabled(&self, id: &str) -> bool {
        self.wapps
            .read()
            .unwrap_or_else(|e| {
                tracing::warn!(error = %e, "wapp registry read lock poisoned; recovering");
                e.into_inner()
            })
            .get(id)
            .is_some_and(|p| p.enabled)
    }

    /// Register an installed package in memory and persist its enabled state.
    pub fn register(&self, manifest: WappManifest, enabled: bool) -> Result<(), String> {
        let dir = self.checked_wapp_dir(&manifest.id)?;
        persist_enabled_marker(&dir, enabled)?;

        self.wapps
            .write()
            .map_err(|e| format!("wapp registry poisoned: {e}"))?
            .insert(manifest.id.clone(), LoadedWapp { manifest, enabled });
        Ok(())
    }

    /// Toggle enable state: updates the `.enabled` marker on disk and the
    /// in-memory registry. Disabled wapps stay listed with `enabled=false`.
    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<(), String> {
        let dir = self.checked_wapp_dir(id)?;
        if !self
            .wapps
            .read()
            .map_err(|e| format!("wapp registry poisoned: {e}"))?
            .contains_key(id)
        {
            return Err(format!("wapp not found: {id}"));
        }

        persist_enabled_marker(&dir, enabled)?;

        let mut map = self
            .wapps
            .write()
            .map_err(|e| format!("wapp registry poisoned: {e}"))?;
        if let Some(loaded) = map.get_mut(id) {
            loaded.enabled = enabled;
        }
        Ok(())
    }

    /// Remove a wapp: deletes its directory (including `.enabled` and
    /// `.storage.json`) and unregisters it.
    pub fn remove(&self, id: &str) -> Result<(), String> {
        let dir = self.checked_wapp_dir(id)?;
        if !self
            .wapps
            .read()
            .map_err(|e| format!("wapp registry poisoned: {e}"))?
            .contains_key(id)
        {
            return Err(format!("wapp not found: {id}"));
        }

        if dir.is_dir() {
            fs::remove_dir_all(&dir).map_err(|e| format!("remove wapp dir {id}: {e}"))?;
        }

        self.wapps
            .write()
            .map_err(|e| format!("wapp registry poisoned: {e}"))?
            .remove(id);
        Ok(())
    }
}

fn persist_enabled_marker(wapp_dir: &Path, enabled: bool) -> Result<(), String> {
    let marker = wapp_dir.join(ENABLED_MARKER_FILE);
    if enabled {
        fs::create_dir_all(wapp_dir).map_err(|e| format!("create wapp dir: {e}"))?;
        fs::write(&marker, b"1\n").map_err(|e| format!("write enabled marker: {e}"))?;
    } else if marker.exists() && fs::remove_file(&marker).is_err() {
        return Err(format!(
            "remove enabled marker failed: {}",
            marker.display()
        ));
    }
    Ok(())
}
