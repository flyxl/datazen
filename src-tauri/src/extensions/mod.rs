//! Runtime UI/theme extension base: registry over installed extension packages.
//!
//! Extensions live in `{appData}/plugins/{id}/` (folder name == `manifest.id`).
//! [`ExtensionManager`] loads every valid package at startup, tracks enabled
//! state via a `.enabled` marker file, and is shared through `AppState`.
pub mod install;
pub mod manifest;
pub mod protocol;
pub mod storage;

#[cfg(test)]
mod integration_tests;

// F9: keeps the E2E sample plugin package (e2e/fixtures/sample-plugin) valid.
#[cfg(test)]
mod fixture_tests;

#[cfg(test)]
mod protocol_security_tests;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

// Re-exported for the IPC layer and the `datazen://` asset service (F2);
// consumed via `crate::extensions::…` paths, so unused-import lint is expected.
#[allow(unused_imports)]
pub use manifest::{
    allowed_extension_file_ext, is_valid_extension_id, parse_manifest, validate_extension_dir,
    validate_manifest, Contributions, ExtensionManifest, PageContribution, Permission,
    ThemeContribution, MAX_EXTENSION_FILES, MAX_EXTENSION_UNCOMPRESSED,
};
#[allow(unused_imports)]
pub use protocol::{handle_datazen_request, parse_datazen_uri, EXTENSIONS_OPEN_PAGE_EVENT};
pub use storage::{storage_get, storage_remove, storage_set};

/// Host-side runtime extension API version; packages must declare
/// `apiVersion == EXTENSION_API_VERSION` to load.
pub const EXTENSION_API_VERSION: u32 = 2;

/// Marker file inside an extension directory; presence means "enabled".
pub const ENABLED_MARKER_FILE: &str = ".enabled";

#[derive(Debug, Clone)]
pub struct LoadedExtension {
    pub manifest: ExtensionManifest,
    pub enabled: bool,
}

/// Shared extension registry. All methods are thread-safe; disk writes are small
/// marker-file operations.
#[derive(Debug)]
pub struct ExtensionManager {
    extensions_dir: PathBuf,
    extensions: RwLock<HashMap<String, LoadedExtension>>,
}

impl ExtensionManager {
    pub fn new(extensions_dir: PathBuf) -> Self {
        Self {
            extensions_dir,
            extensions: RwLock::new(HashMap::new()),
        }
    }

    /// Root directory that holds one sub-directory per installed extension.
    pub fn extensions_dir(&self) -> &Path {
        &self.extensions_dir
    }

    /// Directory of an installed extension (id must be validated first).
    pub fn plugin_dir(&self, id: &str) -> PathBuf {
        self.extensions_dir.join(id)
    }

    fn checked_plugin_dir(&self, id: &str) -> Result<PathBuf, String> {
        if !is_valid_extension_id(id) {
            return Err(format!("invalid extension id: {id}"));
        }
        Ok(self.plugin_dir(id))
    }

    /// Scan `{plugins_dir}` and register every valid package. Invalid or
    /// foreign directories are skipped with a warning. Staging/backup entries
    /// (dot-prefixed) are ignored. Returns the number of loaded extensions.
    pub fn load_from_disk(&self) -> usize {
        let mut map = self
            .extensions
            .write()
            .expect("extension registry poisoned");
        map.clear();

        let Ok(entries) = fs::read_dir(&self.extensions_dir) else {
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

            match validate_extension_dir(&path) {
                Ok(manifest) => {
                    let enabled = self
                        .plugin_dir(&manifest.id)
                        .join(ENABLED_MARKER_FILE)
                        .exists();
                    tracing::debug!(extension = %manifest.id, enabled, "loaded ui extension");
                    map.insert(manifest.id.clone(), LoadedExtension { manifest, enabled });
                    loaded += 1;
                }
                Err(e) => {
                    tracing::warn!(extension = %name, error = %e, "skipping invalid ui extension");
                }
            }
        }

        loaded
    }

    /// Snapshot of all registered extensions, sorted by name then id.
    pub fn list(&self) -> Vec<LoadedExtension> {
        let map = self.extensions.read().expect("extension registry poisoned");
        let mut extensions: Vec<LoadedExtension> = map.values().cloned().collect();
        extensions.sort_by(|a, b| {
            a.manifest
                .name
                .cmp(&b.manifest.name)
                .then_with(|| a.manifest.id.cmp(&b.manifest.id))
        });
        extensions
    }

    pub fn get(&self, id: &str) -> Option<LoadedExtension> {
        self.extensions
            .read()
            .expect("extension registry poisoned")
            .get(id)
            .cloned()
    }

    pub fn manifest(&self, id: &str) -> Option<ExtensionManifest> {
        self.get(id).map(|p| p.manifest)
    }

    pub fn is_enabled(&self, id: &str) -> bool {
        self.extensions
            .read()
            .expect("extension registry poisoned")
            .get(id)
            .is_some_and(|p| p.enabled)
    }

    /// Register an installed package in memory and persist its enabled state.
    pub fn register(&self, manifest: ExtensionManifest, enabled: bool) -> Result<(), String> {
        let dir = self.checked_plugin_dir(&manifest.id)?;
        persist_enabled_marker(&dir, enabled)?;

        self.extensions
            .write()
            .expect("extension registry poisoned")
            .insert(manifest.id.clone(), LoadedExtension { manifest, enabled });
        Ok(())
    }

    /// Toggle enable state: updates the `.enabled` marker on disk and the
    /// in-memory registry. Disabled extensions stay listed with `enabled=false`.
    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<(), String> {
        let dir = self.checked_plugin_dir(id)?;
        if !self
            .extensions
            .read()
            .expect("extension registry poisoned")
            .contains_key(id)
        {
            return Err(format!("extension not found: {id}"));
        }

        persist_enabled_marker(&dir, enabled)?;

        let mut map = self
            .extensions
            .write()
            .expect("extension registry poisoned");
        if let Some(loaded) = map.get_mut(id) {
            loaded.enabled = enabled;
        }
        Ok(())
    }

    /// Remove an extension: deletes its directory (including `.enabled` and
    /// `.storage.json`) and unregisters it.
    pub fn remove(&self, id: &str) -> Result<(), String> {
        let dir = self.checked_plugin_dir(id)?;
        if !self
            .extensions
            .read()
            .expect("extension registry poisoned")
            .contains_key(id)
        {
            return Err(format!("extension not found: {id}"));
        }

        if dir.is_dir() {
            fs::remove_dir_all(&dir).map_err(|e| format!("remove extension dir {id}: {e}"))?;
        }

        self.extensions
            .write()
            .expect("extension registry poisoned")
            .remove(id);
        Ok(())
    }
}

fn persist_enabled_marker(plugin_dir: &Path, enabled: bool) -> Result<(), String> {
    let marker = plugin_dir.join(ENABLED_MARKER_FILE);
    if enabled {
        fs::create_dir_all(plugin_dir).map_err(|e| format!("create extension dir: {e}"))?;
        fs::write(&marker, b"1\n").map_err(|e| format!("write enabled marker: {e}"))?;
    } else if marker.exists() && fs::remove_file(&marker).is_err() {
        return Err(format!(
            "remove enabled marker failed: {}",
            marker.display()
        ));
    }
    Ok(())
}
