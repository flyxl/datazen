//! Port of `packages/frontend/src/lib/structureEditor/isControlEnabled.ts`.
//!
//! React's body is `caps[control] === true`: the boolean-valued capability
//! flags map one-to-one to their `StructureCapabilities` fields. The non-boolean
//! fields (`alterStrategy`, `dialectId`, `indexMethods`) are not reachable
//! through `StructureCapabilityFlag`, so a plain `match` is the exact Rust
//! equivalent — no coercion, no truthiness, no default.
//!
//! T036 inlined this helper in `control_hints.rs` because no separate output
//! file existed yet. It now lives here as the single implementation;
//! `control_hints` re-exports it for its existing callers.

use super::{StructureCapabilities, StructureCapabilityFlag};

/// Port of `isControlEnabled(caps, control)`.
// consumers: control_hints::cap_enabled and the future structure-editor view;
// delete this allow when both are gone.
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
