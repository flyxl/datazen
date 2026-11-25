//! T160 independent audit (tester): the compile-time form determination —
//! the constant's value, its agreement with the capability tables, and
//! the consumer branch shape (a compile-time fold, never a runtime
//! probe).

use datazen_ui::platform::capabilities::{
    DESKTOP_CAPABILITIES, WEB_CAPABILITIES, WEB_SETTINGS_SECTIONS,
};
use datazen_ui::platform::env::IS_DESKTOP_BUILD;

#[test]
fn audit_the_fold_is_desktop_and_the_tables_agree_with_it() {
    // The React semantic: TAURI_ENV_PLATFORM !== undefined at build time.
    // The native crate folds it true; the web capability table is the
    // only thing a false fold would read, and it exists for the future
    // web target.
    assert!(IS_DESKTOP_BUILD);
    let sections = if IS_DESKTOP_BUILD {
        DESKTOP_CAPABILITIES.settings_sections
    } else {
        WEB_SETTINGS_SECTIONS
    };
    assert!(
        sections.len() > WEB_SETTINGS_SECTIONS.len(),
        "the desktop fold selects the larger section set (the consumer branch shape)"
    );
}

#[test]
fn audit_the_two_forms_never_share_the_version_source() {
    // The form determination picks between exactly the two tables; the
    // desktop form reads the host app version, the web form the build
    // constant — no http probing on either side.
    assert_eq!(
        DESKTOP_CAPABILITIES.version_source,
        datazen_ui::platform::capabilities::VersionSource::HostApp
    );
    assert_eq!(
        WEB_CAPABILITIES.version_source,
        datazen_ui::platform::capabilities::VersionSource::BuildConstant
    );
}
