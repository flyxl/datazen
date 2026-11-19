//! T091: `platform/capabilities.ts`.

use crate::platform::capabilities::{
    desktop_sections_match_settings, DESKTOP_CAPABILITIES, WEB_CAPABILITIES, WEB_SETTINGS_SECTIONS,
};
use crate::platform::capabilities::VersionSource;
use crate::settings_state::SETTINGS_SECTIONS;

#[test]
fn desktop_is_every_section_with_a_host_updater() {
    assert!(desktop_sections_match_settings());
    assert_eq!(DESKTOP_CAPABILITIES.settings_sections.len(), SETTINGS_SECTIONS.len());
    assert!(DESKTOP_CAPABILITIES.updater);
    assert_eq!(DESKTOP_CAPABILITIES.version_source, VersionSource::HostApp);
    assert!(DESKTOP_CAPABILITIES
        .settings_sections
        .contains(&"extensions"));
    assert!(DESKTOP_CAPABILITIES.settings_sections.contains(&"logging"));
}

#[test]
fn web_is_the_five_shared_sections_and_a_build_version() {
    assert_eq!(
        WEB_CAPABILITIES.settings_sections,
        WEB_SETTINGS_SECTIONS
    );
    assert_eq!(WEB_SETTINGS_SECTIONS.len(), 5);
    assert!(!WEB_CAPABILITIES.updater);
    assert_eq!(
        WEB_CAPABILITIES.version_source,
        VersionSource::BuildConstant
    );
    for id in ["logging", "ai", "prompts", "mcpServer", "mcpClient", "tunnels", "extensions"] {
        assert!(
            !WEB_CAPABILITIES.settings_sections.contains(&id),
            "{id} is desktop-only"
        );
    }
}
