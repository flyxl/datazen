//! Last-resolved `--c-surface` for native window bg + webview `initialization_script`.
//!
//! Persisted as `{appData}/surface-bg.json` (not localStorage). A plugin injects
//! the baked color into **every** webview, including the config-created main window.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};
use tauri::plugin::Plugin;
use tauri::{AppHandle, Manager, Runtime};

use crate::store::Store;

pub const DEFAULT_SURFACE_DARK_HEX: &str = "#0f172a";
pub const SURFACE_BG_FILE: &str = "surface-bg.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CachedSurface {
    pub hex: String,
    #[serde(default = "default_dark")]
    pub dark: bool,
}

fn default_dark() -> bool {
    true
}

impl Default for CachedSurface {
    fn default() -> Self {
        Self {
            hex: DEFAULT_SURFACE_DARK_HEX.to_string(),
            dark: true,
        }
    }
}

#[derive(Clone)]
pub struct SurfaceBgCache {
    inner: Arc<RwLock<CachedSurface>>,
    path: PathBuf,
}

impl SurfaceBgCache {
    pub fn load() -> Self {
        let path = Store::default_app_data_dir()
            .map(|d| d.join(SURFACE_BG_FILE))
            .unwrap_or_else(|_| PathBuf::from(SURFACE_BG_FILE));
        Self::load_from(path)
    }

    pub fn load_from(path: PathBuf) -> Self {
        let cached = read_cached(&path).unwrap_or_default();
        Self {
            inner: Arc::new(RwLock::new(cached)),
            path,
        }
    }

    pub fn snapshot(&self) -> CachedSurface {
        self.inner
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn hex(&self) -> String {
        self.snapshot().hex
    }

    pub fn set(&self, hex: &str, dark: bool) -> Result<CachedSurface, String> {
        let hex = normalize_css_hex(hex).ok_or_else(|| format!("invalid surface hex: {hex}"))?;
        let cached = CachedSurface { hex, dark };
        {
            let mut guard = self.inner.write().unwrap_or_else(|e| e.into_inner());
            *guard = cached.clone();
        }
        if let Err(e) = write_cached(&self.path, &cached) {
            tracing::warn!(path = %self.path.display(), error = %e, "failed to persist surface-bg.json");
            return Err(e);
        }
        Ok(cached)
    }

    pub fn initialization_script(&self) -> String {
        boot_script(&self.snapshot())
    }
}

pub struct SurfaceBootPlugin {
    cache: SurfaceBgCache,
}

impl SurfaceBootPlugin {
    pub fn new(cache: SurfaceBgCache) -> Self {
        Self { cache }
    }
}

impl<R: Runtime> Plugin<R> for SurfaceBootPlugin {
    fn name(&self) -> &'static str {
        "surface-boot"
    }

    fn initialize(
        &mut self,
        app: &AppHandle<R>,
        _config: serde_json::Value,
    ) -> Result<(), Box<dyn std::error::Error>> {
        app.manage(self.cache.clone());
        Ok(())
    }

    fn initialization_script(&self) -> Option<String> {
        Some(self.cache.initialization_script())
    }
}

/// Parse `#rgb` / `#rrggbb` into RGB. Rejects anything else (no `rgb()`, no alpha).
pub fn parse_css_hex(s: &str) -> Option<(u8, u8, u8)> {
    let hex = s.trim().strip_prefix('#')?;
    match hex.len() {
        3 => {
            let r = u8::from_str_radix(&hex[0..1].repeat(2), 16).ok()?;
            let g = u8::from_str_radix(&hex[1..2].repeat(2), 16).ok()?;
            let b = u8::from_str_radix(&hex[2..3].repeat(2), 16).ok()?;
            Some((r, g, b))
        }
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            Some((r, g, b))
        }
        _ => None,
    }
}

pub fn normalize_css_hex(s: &str) -> Option<String> {
    let (r, g, b) = parse_css_hex(s)?;
    Some(format!("#{r:02x}{g:02x}{b:02x}"))
}

fn read_cached(path: &Path) -> Option<CachedSurface> {
    let raw = fs::read_to_string(path).ok()?;
    let parsed: CachedSurface = serde_json::from_str(&raw).ok()?;
    let hex = normalize_css_hex(&parsed.hex)?;
    Some(CachedSurface {
        hex,
        dark: parsed.dark,
    })
}

fn write_cached(path: &Path, cached: &CachedSurface) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string(cached).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json).map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| e.to_string())?;
    Ok(())
}

fn boot_script(cached: &CachedSurface) -> String {
    let dark = if cached.dark { "true" } else { "false" };
    let hex = &cached.hex;
    format!(
        "try{{document.documentElement.classList.toggle('dark',{dark});document.documentElement.style.backgroundColor='{hex}';}}catch(e){{}}\
try{{if(window.__TAURI_INTERNALS__&&window.__TAURI_INTERNALS__.invoke){{window.__TAURI_INTERNALS__.invoke('plugin:window|set_background_color',{{color:'{hex}'}}).catch(function(){{}});}}}}catch(e){{}}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("datazen-surface-{name}-{nanos}.json"))
    }

    #[test]
    fn parse_and_normalize_hex() {
        assert_eq!(parse_css_hex("#0f172a"), Some((0x0f, 0x17, 0x2a)));
        assert_eq!(normalize_css_hex("#ABC"), Some("#aabbcc".into()));
        assert_eq!(normalize_css_hex("#fff"), Some("#ffffff".into()));
        assert!(normalize_css_hex("not-a-color").is_none());
        assert!(normalize_css_hex("#gg0000").is_none());
        assert!(normalize_css_hex("rgb(1,2,3)").is_none());
    }

    #[test]
    fn load_missing_file_uses_default() {
        let path = temp_path("missing");
        let cache = SurfaceBgCache::load_from(path);
        assert_eq!(cache.hex(), DEFAULT_SURFACE_DARK_HEX);
        assert!(cache.snapshot().dark);
    }

    #[test]
    fn set_persists_and_bakes_script() {
        let path = temp_path("persist");
        let cache = SurfaceBgCache::load_from(path.clone());
        cache.set("#1A0A2E", false).unwrap();
        assert_eq!(cache.hex(), "#1a0a2e");
        assert!(!cache.snapshot().dark);

        let on_disk: CachedSurface =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(on_disk.hex, "#1a0a2e");
        assert!(!on_disk.dark);

        let script = cache.initialization_script();
        assert!(script.contains("#1a0a2e"));
        assert!(script.contains("toggle('dark',false)"));
        assert!(script.contains("plugin:window|set_background_color"));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn set_rejects_unsafe_color() {
        let path = temp_path("bad");
        let cache = SurfaceBgCache::load_from(path);
        assert!(cache.set("javascript:alert(1)", true).is_err());
        assert_eq!(cache.hex(), DEFAULT_SURFACE_DARK_HEX);
    }

    #[test]
    fn load_ignores_corrupt_or_unsafe_file() {
        let path = temp_path("corrupt");
        fs::write(&path, r#"{"hex":"javascript:alert(1)","dark":false}"#).unwrap();
        let cache = SurfaceBgCache::load_from(path.clone());
        assert_eq!(cache.hex(), DEFAULT_SURFACE_DARK_HEX);
        assert!(cache.snapshot().dark);
        let _ = fs::remove_file(path);
    }
}
