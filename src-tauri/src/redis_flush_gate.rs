//! Sync driver settings from host AppSettings into linked driver crates.
//!
//! Each driver crate exposes `SETTINGS_KEY` and a `set_settings_*` setter.
//! The host iterates known driver keys generically — it does NOT hard-code
//! plugin ids; the driver owns its key identity.
//!
//! `driver-redis` is injected by `resolve-drivers.mjs`; stub Cargo.toml may not
//! declare it, so unexpected_cfgs is allowed for this module.
#![allow(unexpected_cfgs)]

use crate::store::AppSettings;

/// Mirror `pluginSettings[key].allowFlush` into the redis driver process gate.
/// The key is read from `datazen_driver_redis::SETTINGS_KEY` so the host
/// does not hard-code the plugin id.
pub fn sync_from_settings(settings: &AppSettings) {
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
