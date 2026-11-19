//! Independent audit of T091.
//!
//! Web keeps `behavior` and drops `logging`. Desktop order starts at
//! `general` and ends at `extensions`. Neither table uses `http`.

use crate::platform::capabilities::{
    DESKTOP_CAPABILITIES, WEB_CAPABILITIES, VersionSource,
};

#[test]
fn audit_web_keeps_behavior_and_drops_logging() {
    assert!(WEB_CAPABILITIES.settings_sections.contains(&"behavior"));
    assert!(!WEB_CAPABILITIES.settings_sections.contains(&"logging"));
    assert_eq!(WEB_CAPABILITIES.settings_sections[0], "general");
    assert_eq!(
        DESKTOP_CAPABILITIES.settings_sections.last().copied(),
        Some("extensions")
    );
    assert_ne!(DESKTOP_CAPABILITIES.version_source, VersionSource::Http);
    assert_ne!(WEB_CAPABILITIES.version_source, VersionSource::Http);
    assert!(DESKTOP_CAPABILITIES.updater);
    assert!(!WEB_CAPABILITIES.updater);
}
