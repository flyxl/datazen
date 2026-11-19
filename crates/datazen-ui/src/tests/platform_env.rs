//! T160: `platform/env.ts` — the compile-time form determination.

use crate::platform::env::IS_DESKTOP_BUILD;
use crate::platform::capabilities::{DESKTOP_CAPABILITIES, WEB_CAPABILITIES};

#[test]
fn the_native_build_is_the_desktop_form() {
    // The whole crate compiles as the desktop binary: the constant folds
    // true at compile time and every desktop-only consumer path is live.
    assert!(IS_DESKTOP_BUILD);
    // The form determination and the capability tables agree: the
    // desktop form is the one with the host updater and every section.
    assert!(DESKTOP_CAPABILITIES.updater);
    assert!(!WEB_CAPABILITIES.updater);
}
