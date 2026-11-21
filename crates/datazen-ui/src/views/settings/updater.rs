//! Carried port of `packages/frontend/src/lib/updater.ts` (T638's,
//! segment 2 — carried until it lands; the UpdateSection port T464
//! needs its pure core).
//!
//! Channel/variant/manifest logic is pure build-constant reasoning
//! and lives here; the plugin-updater *flows* (check / download /
//! install / relaunch) are app-owned and stay host slots on the
//! section. The React module reads its constants (`DATAZEN_VARIANT`,
//! `DATAZEN_UPDATER_CHANNEL`, `IS_DESKTOP_BUILD`) from the generated
//! build config; the carried port takes them as arguments so the
//! host injects the build's values.
//!
//! ## What maps where
//!
//! - `UpdateChannel` ('auto' | 'manual' | 'none') is
//!   [`UpdateChannel`]; `getUpdateChannel()` is
//!   [`get_update_channel`] over the two constants; `currentVariant`
//!   is the host-injected string (no function needed);
//!   `isUpdaterSupported()` is [`is_updater_supported`].
//! - The manifest defence (`normalizeSku`, `readManifestVariant`,
//!   `manifestBelongsToBuild`) is [`normalize_sku`] /
//!   [`read_manifest_variant`] / [`manifest_belongs_to_build`] —
//!   the second line of defence against a SKU reading another
//!   variant's manifest, including the Basic grandfather rule (a
//!   manifest with no `variant` is Basic's, and only Basic's).
//! - `maybeCheckOnStartup`'s gate (the diagnosable skip) is
//!   [`startup_check_should_run`]; the `console.info`/`console.warn`
//!   halves are the host's logging seam.
//! - `UpdateProgress` / `UpdateCheckResult` (platform/contracts)
//!   are [`UpdateProgress`] / [`UpdateCheckResult`]; the three
//!   port accessors (`checkForUpdates`,
//!   `downloadAndInstallUpdate`, `relaunchToApplyUpdate`) are the
//!   section's host slots.
//!
//! Known deltas: the build constants are arguments, not ambient
//! imports; everything else is the React file's verbatim.

// Carried for T464 (and T638 when it lands); no production consumer
// yet — this crate's tests are the callers meanwhile.
#![allow(dead_code)]

use serde_json::Value;

/// How this build receives updates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UpdateChannel {
    /// This SKU publishes signed updater artifacts plus its own
    /// manifest, so the in-app updater is safe to run.
    Auto,
    /// A desktop build whose SKU publishes no channel; the user
    /// downloads the installer for that SKU from GitHub Releases.
    Manual,
    /// A form without an updater (browser dev / web), so there is
    /// nothing to show.
    None,
}

impl UpdateChannel {
    /// The React string literal.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            UpdateChannel::Auto => "auto",
            UpdateChannel::Manual => "manual",
            UpdateChannel::None => "none",
        }
    }
}

/// `getUpdateChannel()`: the browser-dev run of the desktop bundle
/// folds to `none` (ruling #5); a desktop build with a published
/// channel is `auto`, without one `manual`.
pub(crate) fn get_update_channel(is_desktop_build: bool, has_updater_channel: bool) -> UpdateChannel {
    if !is_desktop_build {
        return UpdateChannel::None;
    }
    if has_updater_channel {
        UpdateChannel::Auto
    } else {
        UpdateChannel::Manual
    }
}

/// `isUpdaterSupported()`: false when the in-app updater must not
/// run.
pub(crate) fn is_updater_supported(channel: UpdateChannel) -> bool {
    channel == UpdateChannel::Auto
}

/// `normalizeSku(value)`: trim, lowercase, strip the leading dashes
/// so `all` and `-all` compare equal.
pub(crate) fn normalize_sku(value: &str) -> String {
    value.trim().to_lowercase().trim_start_matches('-').to_owned()
}

/// `readManifestVariant(rawJson)`: the manifest's top-level
/// `variant`; missing, non-string or blank all collapse to `None`.
pub(crate) fn read_manifest_variant(raw_json: &Value) -> Option<String> {
    let value = raw_json.get("variant")?;
    let as_str = value.as_str()?;
    let trimmed = as_str.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

/// `manifestBelongsToBuild(rawJson, variant)`: whether this manifest
/// belongs to the build that is reading it. A manifest with no
/// `variant` is Basic's (Basic keeps the manifest name from earlier
/// releases); variant manifests only came into existence with
/// per-SKU channels, so a missing field there can only mean the
/// manifest belongs to someone else.
pub(crate) fn manifest_belongs_to_build(raw_json: &Value, variant: &str) -> bool {
    match read_manifest_variant(raw_json) {
        None => normalize_sku(variant) == "basic",
        Some(found) => normalize_sku(&found) == normalize_sku(variant),
    }
}

/// `maybeCheckOnStartup`'s gate: the silent startup check runs only
/// when the setting is on and the channel is `auto` (the skip is
/// diagnosable on purpose — the host logs the channel and variant).
pub(crate) fn startup_check_should_run(enabled: bool, channel: UpdateChannel) -> bool {
    enabled && channel == UpdateChannel::Auto
}

/// `UpdateProgress` (platform/contracts): the flow's phase plus the
/// download counters (bytes).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct UpdateProgress {
    pub(crate) phase: UpdatePhase,
    pub(crate) downloaded: u64,
    /// `None` (or zero) — an indeterminate download.
    pub(crate) total: Option<u64>,
    pub(crate) version: String,
}

/// The progress phases.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UpdatePhase {
    Checking,
    Downloading,
    Installing,
    Done,
}

/// `UpdateCheckResult` (platform/contracts): the four statuses —
/// `checkForUpdates` answers `UpToDate` / `Available` / `Error`,
/// `downloadAndInstallUpdate` answers `UpToDate` / `Installed` /
/// `Error` (the type is shared, so the impossible arms simply never
/// arrive).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum UpdateCheckResult {
    UpToDate,
    Available { version: String },
    Installed { version: String },
    Error { message: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_channel_follows_the_build_constants() {
        // A browser-dev run folds to none whatever the channel.
        assert_eq!(get_update_channel(false, true), UpdateChannel::None);
        assert_eq!(get_update_channel(false, false), UpdateChannel::None);
        // A desktop build with a channel is auto, without one manual.
        assert_eq!(get_update_channel(true, true), UpdateChannel::Auto);
        assert_eq!(get_update_channel(true, false), UpdateChannel::Manual);
        // Only auto supports the in-app updater.
        assert!(is_updater_supported(UpdateChannel::Auto));
        assert!(!is_updater_supported(UpdateChannel::Manual));
        assert!(!is_updater_supported(UpdateChannel::None));
        // The wire literals.
        assert_eq!(UpdateChannel::Auto.as_str(), "auto");
        assert_eq!(UpdateChannel::Manual.as_str(), "manual");
        assert_eq!(UpdateChannel::None.as_str(), "none");
    }

    #[test]
    fn the_startup_gate_needs_the_setting_and_the_channel() {
        assert!(startup_check_should_run(true, UpdateChannel::Auto));
        assert!(!startup_check_should_run(false, UpdateChannel::Auto));
        assert!(!startup_check_should_run(true, UpdateChannel::Manual));
        assert!(!startup_check_should_run(true, UpdateChannel::None));
    }

    #[test]
    fn sku_names_normalize_for_comparison() {
        assert_eq!(normalize_sku(" All "), "all");
        assert_eq!(normalize_sku("-all"), "all");
        assert_eq!(normalize_sku("--Akulaku"), "akulaku");
        assert_eq!(normalize_sku("Basic"), "basic");
    }

    #[test]
    fn the_manifest_variant_read_collapses_the_junk() {
        assert_eq!(read_manifest_variant(&json!({"variant": "all"})), Some("all".to_owned()));
        assert_eq!(read_manifest_variant(&json!({"variant": "  "})), None);
        assert_eq!(read_manifest_variant(&json!({"variant": 7})), None);
        assert_eq!(read_manifest_variant(&json!({})), None);
        assert_eq!(read_manifest_variant(&Value::Null), None);
    }

    #[test]
    fn the_manifest_defence_splits_on_the_grandfather_rule() {
        // A matching variant belongs.
        assert!(manifest_belongs_to_build(&json!({"variant": "all"}), "all"));
        assert!(manifest_belongs_to_build(&json!({"variant": "-All"}), "all"));
        // Another SKU's manifest does not.
        assert!(!manifest_belongs_to_build(&json!({"variant": "akulaku"}), "all"));
        // A manifest with no variant is Basic's — and only Basic's.
        assert!(manifest_belongs_to_build(&json!({}), "basic"));
        assert!(!manifest_belongs_to_build(&json!({}), "all"));
        // Only LEADING dashes strip (JS /^-+/), so the trailing one
        // keeps this from being basic.
        assert!(!manifest_belongs_to_build(&json!({}), "-basic- "));
        assert!(manifest_belongs_to_build(&json!({}), "-basic"));
    }
}
