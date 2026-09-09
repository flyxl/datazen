//! Sync driver settings from host AppSettings into linked driver crates.
//!
//! Each driver crate exposes `SETTINGS_KEY` and a settings applicator.
//! The host reads the key from the driver crate so plugin ids are not
//! hard-coded here.
//!
//! Features `driver-redis` / `driver-jdbc` are injected by `resolve-drivers.mjs`;
//! stub Cargo.toml may not declare them, so unexpected_cfgs is allowed.
#![allow(unexpected_cfgs)]

use crate::store::AppSettings;

/// Mirror `pluginSettings` into linked driver crates (boot + every save_settings).
pub fn sync_from_settings(settings: &AppSettings) {
    sync_redis(settings);
    sync_jdbc(settings);
}

fn sync_redis(settings: &AppSettings) {
    #[cfg(feature = "driver-redis")]
    {
        let key = datazen_driver_redis::SETTINGS_KEY;
        let allow = settings
            .plugin_settings
            .get(key)
            .and_then(|v| v.get("allowFlush"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        datazen_driver_redis::set_settings_allow_flush(allow);
    }
    #[cfg(not(feature = "driver-redis"))]
    {
        let _ = settings;
    }
}

fn sync_jdbc(settings: &AppSettings) {
    #[cfg(feature = "driver-jdbc")]
    {
        let key = datazen_driver_jdbc::SETTINGS_KEY;
        let value = settings
            .plugin_settings
            .get(key)
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        datazen_driver_jdbc::apply_plugin_settings(&value);
    }
    #[cfg(not(feature = "driver-jdbc"))]
    {
        let _ = settings;
    }
}
