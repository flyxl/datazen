//! Sync Redis flush allow-list from host settings into the redis driver crate.
//!
//! `plugin-redis` is injected by `resolve-drivers.mjs`; stub Cargo.toml may not
//! declare it, so unexpected_cfgs is allowed for this module.
#![allow(unexpected_cfgs)]

use crate::store::AppSettings;

/// Mirror `pluginSettings.redis.allowFlush` into the redis driver process gate.
/// No-op when the redis driver is not linked into this build.
pub fn sync_from_settings(settings: &AppSettings) {
    #[cfg(feature = "plugin-redis")]
    {
        let allow = settings
            .plugin_settings
            .get("redis")
            .and_then(|v| v.get("allowFlush"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        datazen_driver_redis::set_settings_allow_flush(allow);
    }
    #[cfg(not(feature = "plugin-redis"))]
    {
        let _ = settings;
    }
}
