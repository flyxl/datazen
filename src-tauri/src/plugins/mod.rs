//! Runtime UI/theme plugin base: registry over installed plugin packages.
//!
//! Plugins live in `{appData}/plugins/{id}/` (folder name == `manifest.id`).
//! [`PluginManager`] loads every valid package at startup, tracks enabled
//! state via a `.enabled` marker file, and is shared through `AppState`.
pub mod install;
pub mod manifest;
pub mod storage;

#[cfg(test)]
mod integration_tests;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

// Re-exported for the IPC layer and the upcoming `datazen://` asset service (F2).
#[allow(unused_imports)]
pub use manifest::{
    allowed_plugin_extension, is_valid_plugin_id, parse_manifest, validate_manifest,
    validate_plugin_dir, Contributions, PageContribution, Permission, PluginManifest,
    ThemeContribution, MAX_PLUGIN_FILES, MAX_PLUGIN_UNCOMPRESSED,
};
pub use storage::{storage_get, storage_remove, storage_set};

/// Host-side runtime plugin API version; packages must declare
/// `apiVersion == PLUGIN_API_VERSION` to load.
pub const PLUGIN_API_VERSION: u32 = 2;

/// Marker file inside a plugin directory; presence means "enabled".
pub const ENABLED_MARKER_FILE: &str = ".enabled";

#[derive(Debug, Clone)]
pub struct LoadedPlugin {
    pub manifest: PluginManifest,
    pub enabled: bool,
}

/// Shared plugin registry. All methods are thread-safe; disk writes are small
/// marker-file operations.
#[derive(Debug)]
pub struct PluginManager {
    plugins_dir: PathBuf,
    plugins: RwLock<HashMap<String, LoadedPlugin>>,
}

impl PluginManager {
    pub fn new(plugins_dir: PathBuf) -> Self {
        Self {
            plugins_dir,
            plugins: RwLock::new(HashMap::new()),
        }
    }

    /// Root directory that holds one sub-directory per installed plugin.
    pub fn plugins_dir(&self) -> &Path {
        &self.plugins_dir
    }

    /// Directory of an installed plugin (id must be validated first).
    pub fn plugin_dir(&self, id: &str) -> PathBuf {
        self.plugins_dir.join(id)
    }

    fn checked_plugin_dir(&self, id: &str) -> Result<PathBuf, String> {
        if !is_valid_plugin_id(id) {
            return Err(format!("invalid plugin id: {id}"));
        }
        Ok(self.plugin_dir(id))
    }

    /// Scan `{plugins_dir}` and register every valid package. Invalid or
    /// foreign directories are skipped with a warning. Staging/backup entries
    /// (dot-prefixed) are ignored. Returns the number of loaded plugins.
    pub fn load_from_disk(&self) -> usize {
        let mut map = self.plugins.write().expect("plugin registry poisoned");
        map.clear();

        let Ok(entries) = fs::read_dir(&self.plugins_dir) else {
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

            match validate_plugin_dir(&path) {
                Ok(manifest) => {
                    let enabled = self
                        .plugin_dir(&manifest.id)
                        .join(ENABLED_MARKER_FILE)
                        .exists();
                    tracing::debug!(plugin = %manifest.id, enabled, "loaded ui plugin");
                    map.insert(manifest.id.clone(), LoadedPlugin { manifest, enabled });
                    loaded += 1;
                }
                Err(e) => {
                    tracing::warn!(plugin = %name, error = %e, "skipping invalid ui plugin");
                }
            }
        }

        loaded
    }

    /// Snapshot of all registered plugins, sorted by name then id.
    pub fn list(&self) -> Vec<LoadedPlugin> {
        let map = self.plugins.read().expect("plugin registry poisoned");
        let mut plugins: Vec<LoadedPlugin> = map.values().cloned().collect();
        plugins.sort_by(|a, b| {
            a.manifest
                .name
                .cmp(&b.manifest.name)
                .then_with(|| a.manifest.id.cmp(&b.manifest.id))
        });
        plugins
    }

    pub fn get(&self, id: &str) -> Option<LoadedPlugin> {
        self.plugins
            .read()
            .expect("plugin registry poisoned")
            .get(id)
            .cloned()
    }

    pub fn manifest(&self, id: &str) -> Option<PluginManifest> {
        self.get(id).map(|p| p.manifest)
    }

    pub fn is_enabled(&self, id: &str) -> bool {
        self.plugins
            .read()
            .expect("plugin registry poisoned")
            .get(id)
            .is_some_and(|p| p.enabled)
    }

    /// Register an installed package in memory and persist its enabled state.
    pub fn register(&self, manifest: PluginManifest, enabled: bool) -> Result<(), String> {
        let dir = self.checked_plugin_dir(&manifest.id)?;
        persist_enabled_marker(&dir, enabled)?;

        self.plugins
            .write()
            .expect("plugin registry poisoned")
            .insert(manifest.id.clone(), LoadedPlugin { manifest, enabled });
        Ok(())
    }

    /// Toggle enable state: updates the `.enabled` marker on disk and the
    /// in-memory registry. Disabled plugins stay listed with `enabled=false`.
    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<(), String> {
        let dir = self.checked_plugin_dir(id)?;
        if !self
            .plugins
            .read()
            .expect("plugin registry poisoned")
            .contains_key(id)
        {
            return Err(format!("plugin not found: {id}"));
        }

        persist_enabled_marker(&dir, enabled)?;

        let mut map = self.plugins.write().expect("plugin registry poisoned");
        if let Some(loaded) = map.get_mut(id) {
            loaded.enabled = enabled;
        }
        Ok(())
    }

    /// Remove a plugin: deletes its directory (including `.enabled` and
    /// `.storage.json`) and unregisters it.
    pub fn remove(&self, id: &str) -> Result<(), String> {
        let dir = self.checked_plugin_dir(id)?;
        if !self
            .plugins
            .read()
            .expect("plugin registry poisoned")
            .contains_key(id)
        {
            return Err(format!("plugin not found: {id}"));
        }

        if dir.is_dir() {
            fs::remove_dir_all(&dir).map_err(|e| format!("remove plugin dir {id}: {e}"))?;
        }

        self.plugins
            .write()
            .expect("plugin registry poisoned")
            .remove(id);
        Ok(())
    }
}

fn persist_enabled_marker(plugin_dir: &Path, enabled: bool) -> Result<(), String> {
    let marker = plugin_dir.join(ENABLED_MARKER_FILE);
    if enabled {
        fs::create_dir_all(plugin_dir).map_err(|e| format!("create plugin dir: {e}"))?;
        fs::write(&marker, b"1\n").map_err(|e| format!("write enabled marker: {e}"))?;
    } else if marker.exists() && fs::remove_file(&marker).is_err() {
        return Err(format!(
            "remove enabled marker failed: {}",
            marker.display()
        ));
    }
    Ok(())
}
