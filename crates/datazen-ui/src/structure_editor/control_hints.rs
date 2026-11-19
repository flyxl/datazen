//! Port of `packages/frontend/src/lib/structureEditor/controlHints.ts`.
//!
//! `controlHints.ts` imports `isControlEnabled` from its sibling module; that
//! helper is inlined here (there is no separate output file for T036). The
//! i18n key is a literal return type in React, so it is a `&'static str`
//! constant on the Rust side.

use super::{StructureCapabilities, StructureCapabilityFlag};

/// The only key `controlDisabledKey` can return (`'structEditor.capDisabled'`).
pub(crate) const CONTROL_DISABLED_KEY: &str = "structEditor.capDisabled";

/// Port of `controlDisabledKey(_control)` — the argument is intentionally
/// ignored, exactly as in React.
// consumers: the future structure-editor view; delete this allow when it lands.
#[allow(dead_code)]
pub(crate) fn control_disabled_key(_control: StructureCapabilityFlag) -> &'static str {
    CONTROL_DISABLED_KEY
}

/// Port of `isControlEnabled.ts`: `caps[control] === true`. Every flag maps to
/// its own boolean field; the three non-boolean capability fields are not
/// reachable through `StructureCapabilityFlag`.
// consumers: the future structure-editor view; delete this allow when it lands.
#[allow(dead_code)]
pub(crate) fn is_control_enabled(
    caps: &StructureCapabilities,
    control: StructureCapabilityFlag,
) -> bool {
    match control {
        StructureCapabilityFlag::CreateTable => caps.create_table,
        StructureCapabilityFlag::AddColumn => caps.add_column,
        StructureCapabilityFlag::DropColumn => caps.drop_column,
        StructureCapabilityFlag::RenameColumn => caps.rename_column,
        StructureCapabilityFlag::AlterType => caps.alter_type,
        StructureCapabilityFlag::AlterNullability => caps.alter_nullability,
        StructureCapabilityFlag::AlterDefault => caps.alter_default,
        StructureCapabilityFlag::AlterPrimaryKey => caps.alter_primary_key,
        StructureCapabilityFlag::ReorderColumn => caps.reorder_column,
        StructureCapabilityFlag::Comment => caps.comment,
        StructureCapabilityFlag::CreateIndex => caps.create_index,
        StructureCapabilityFlag::DropIndex => caps.drop_index,
        StructureCapabilityFlag::RebuildIndex => caps.rebuild_index,
        StructureCapabilityFlag::IndexType => caps.index_type,
        StructureCapabilityFlag::IndexInclude => caps.index_include,
        StructureCapabilityFlag::IndexFilter => caps.index_filter,
        StructureCapabilityFlag::IndexComment => caps.index_comment,
    }
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
