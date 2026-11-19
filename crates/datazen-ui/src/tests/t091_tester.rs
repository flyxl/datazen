//! Independent T091 retest. Section ids are the `SETTINGS_SECTIONS` array
//! in `windows/settings/settingsSections.ts`, in source order. Web is the
//! five-id set written in `platform/capabilities.ts`.

use crate::platform::capabilities::{
    desktop_sections_match_settings, DESKTOP_CAPABILITIES, WEB_CAPABILITIES, VersionSource,
};
use crate::settings_state::SETTINGS_SECTIONS;

const REACT_SETTINGS_IDS: [&str; 12] = [
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

const REACT_WEB_IDS: [&str; 5] = ["general", "appearance", "dataBrowsing", "editor", "behavior"];

#[test]
fn desktop_sections_follow_settings_sections_source_order() {
    assert_eq!(DESKTOP_CAPABILITIES.settings_sections, &REACT_SETTINGS_IDS);
    let from_state: Vec<&str> = SETTINGS_SECTIONS.iter().map(|section| section.id).collect();
    assert_eq!(from_state, REACT_SETTINGS_IDS);
    assert!(desktop_sections_match_settings());
    assert!(DESKTOP_CAPABILITIES.updater);
    assert_eq!(DESKTOP_CAPABILITIES.version_source, VersionSource::HostApp);
}

#[test]
fn web_sections_are_the_five_shared_ids_without_an_updater() {
    assert_eq!(WEB_CAPABILITIES.settings_sections, &REACT_WEB_IDS);
    assert!(!WEB_CAPABILITIES.updater);
    assert_eq!(WEB_CAPABILITIES.version_source, VersionSource::BuildConstant);
    for id in ["logging", "ai", "prompts", "mcpServer", "mcpClient", "tunnels", "extensions"] {
        assert!(
            !WEB_CAPABILITIES.settings_sections.contains(&id),
            "{id} is desktop-only"
        );
    }
}
