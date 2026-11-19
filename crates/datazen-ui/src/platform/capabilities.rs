//! Static capability tables from `platform/capabilities.ts` (T091).
//!
//! Capabilities are a compile-time property of the form factor. Consumers
//! branch on these tables, never on a runtime form check. Desktop sections
//! are every id in [`crate::settings_state::SETTINGS_SECTIONS`] (React
//! `ALL_SETTINGS_SECTIONS`). Web is the five shared sections only.

#![allow(dead_code)]

use crate::settings_state::SETTINGS_SECTIONS;

/// `PlatformCapabilities.versionSource`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VersionSource {
    HostApp,
    BuildConstant,
    Http,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PlatformCapabilities {
    pub settings_sections: &'static [&'static str],
    pub updater: bool,
    pub version_source: VersionSource,
}

/// `WEB_SETTINGS_SECTIONS`, in the order React inserts them.
pub(crate) const WEB_SETTINGS_SECTIONS: &[&str] = &[
    "general",
    "appearance",
    "dataBrowsing",
    "editor",
    "behavior",
];

/// `DESKTOP_CAPABILITIES`.
pub(crate) const DESKTOP_CAPABILITIES: PlatformCapabilities = PlatformCapabilities {
    settings_sections: DESKTOP_SETTINGS_SECTIONS,
    updater: true,
    version_source: VersionSource::HostApp,
};

/// `WEB_CAPABILITIES`.
pub(crate) const WEB_CAPABILITIES: PlatformCapabilities = PlatformCapabilities {
    settings_sections: WEB_SETTINGS_SECTIONS,
    updater: false,
    version_source: VersionSource::BuildConstant,
};

/// Ids of [`SETTINGS_SECTIONS`], which is how React builds
/// `ALL_SETTINGS_SECTIONS`.
const DESKTOP_SETTINGS_SECTIONS: &[&str] = &[
    "general",
    "appearance",
    "dataBrowsing",
    "editor",
    "behavior",
    "logging",
    "ai",
    "prompts",
    "mcpServer",
    "mcpClient",
    "tunnels",
    "extensions",
];

/// True when the desktop table still lists every settings section, in order.
pub(crate) fn desktop_sections_match_settings() -> bool {
    SETTINGS_SECTIONS.len() == DESKTOP_SETTINGS_SECTIONS.len()
        && SETTINGS_SECTIONS
            .iter()
            .zip(DESKTOP_SETTINGS_SECTIONS)
            .all(|(section, id)| section.id == *id)
}
