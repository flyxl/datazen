//! Port of `packages/frontend/src/lib/structureEditor/controlHints.ts`.
//!
//! `controlHints.ts` imports `isControlEnabled` from its sibling module. T036
//! inlined that helper because no separate output file existed; T124 landed
//! `is_control_enabled.rs`, so this module now re-exports the single
//! implementation instead of keeping a second copy. The i18n key is a literal
//! return type in React, so it is a `&'static str` constant on the Rust side.

use super::{StructureCapabilities, StructureCapabilityFlag};

pub(crate) use super::is_control_enabled::is_control_enabled;

/// The only key `controlDisabledKey` can return (`'structEditor.capDisabled'`).
pub(crate) const CONTROL_DISABLED_KEY: &str = "structEditor.capDisabled";

/// Port of `controlDisabledKey(_control)` — the argument is intentionally
/// ignored, exactly as in React.
// consumers: the future structure-editor view; delete this allow when it lands.
#[allow(dead_code)]
pub(crate) fn control_disabled_key(_control: StructureCapabilityFlag) -> &'static str {
    CONTROL_DISABLED_KEY
}

/// Port of `capEnabled(caps, control)` — a null capability set fails closed.
// consumers: the future structure-editor view; delete this allow when it lands.
#[allow(dead_code)]
pub(crate) fn cap_enabled(
    caps: Option<&StructureCapabilities>,
    control: StructureCapabilityFlag,
) -> bool {
    let Some(caps) = caps else {
        return false;
    };
    is_control_enabled(caps, control)
}
