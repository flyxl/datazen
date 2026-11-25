//! Port of `windows/connection/useTableForeignKeyNavigation.ts` — the FK
//! navigation hook — as pure decision logic (T628).
//!
//! React's hook owns the metadata, the jump, the error, the breadcrumb, the
//! keyboard and the viewport. This port keeps the *decisions*: which cell may be
//! followed and with which values, how the breadcrumb is derived, how the
//! back/forward stack behaves, and how a viewport anchor is chosen. The host
//! owns the state and performs the real tab opens / panel activations / scroll
//! writes; the pure helpers answer with [`JumpDecision`] values and stack
//! entries. No IPC, promise, timer or DOM lives here.
//!
//! The `lib/fkNavigation.ts` helpers and the `fkNavigationStore.ts` stack are
//! ported into [`logic`] and [`history`]; the hook's own wiring (jump, error,
//! breadcrumb) is in this file. `table_fk_breadcrumb::FkOrigin` is reused so the
//! breadcrumb type has one definition.
//!
//! ## Consumer / removal stage
//!
//! No production table host wires this yet; it will own one [`history::FkNavigationMachine`]
//! per session and translate the decisions. Every reusable item carries a
//! per-item `#[allow(dead_code)]` with this stage.

use std::collections::BTreeSet;

use crate::views::table_fk_breadcrumb::FkOrigin;

mod history;
mod logic;
mod model;

pub use history::FkNavigationMachine;
pub use logic::{
    build_fk_filters, find_entry_id_by_nav_key, find_foreign_key, fk_nav_key, index_foreign_keys,
    is_fk_cell_navigable, pick_anchor_row, scroll_top_for_anchor_row, target_label_of,
    NavigableInput, FK_NAV_MAX_DEPTH,
};
pub use model::{
    json_quote, FkFilterVm, FkGroupAffordanceVm, FkIndexVm, FkNavEntryVm, FkNavOriginVm,
    FkNavTargetIdentityVm, FkValue, ForeignKeyInfoVm, GridViewportVm,
};

/// What the hook decides when a cell's arrow is pressed.
#[allow(dead_code)] // decision; consumed by the table host (not wired).
#[derive(Clone, Debug, PartialEq)]
pub enum JumpDecision {
    /// No group, no row, or a NULL in any member — nothing may happen.
    Refuse,
    /// The full foreign key, the group values in pairing order, and the
    /// `${rowIndex}:${columnName}` resolving key.
    Proceed {
        cell_key: String,
        foreign_key: ForeignKeyInfoVm,
        values: Vec<FkValue>,
    },
}

/// React `handleFkJump`'s decision: group lookup → row lookup → whole-group
/// values (a single NULL refuses) → `ForeignKeyInfo` + cell key. The actual
/// `openForeignKeyTarget` call stays with the host.
#[allow(dead_code)] // pure decision; remove with the host wiring.
pub fn resolve_jump(
    column_name: &str,
    row_index: usize,
    rows: &[std::collections::BTreeMap<String, FkValue>],
    index: &FkIndexVm,
) -> JumpDecision {
    let Some(group) = index.get(column_name) else {
        return JumpDecision::Refuse;
    };
    let Some(row) = rows.get(row_index) else {
        return JumpDecision::Refuse;
    };
    let values: Vec<FkValue> = group
        .local_columns
        .iter()
        .map(|column| row.get(column).cloned().unwrap_or(FkValue::Null))
        .collect();
    if values.iter().any(FkValue::is_absent) {
        return JumpDecision::Refuse;
    }
    let foreign_key = ForeignKeyInfoVm {
        name: group.constraint_name.clone(),
        columns: group.local_columns.clone(),
        referenced_table: group.reference_table.clone(),
        referenced_columns: group.target_columns.clone(),
    };
    JumpDecision::Proceed {
        cell_key: format!("{row_index}:{column_name}"),
        foreign_key,
        values,
    }
}

/// React `setResolving(current => new Set(current).add(cellKey))`.
#[allow(dead_code)] // pure set transition; remove with the host wiring.
pub fn resolving_add(
    resolving: &BTreeSet<String>,
    cell_key: &str,
) -> BTreeSet<String> {
    let mut next = resolving.clone();
    next.insert(cell_key.to_owned());
    next
}

/// React's `.finally(() => setResolving(current => { ...delete(cellKey) }))`.
#[allow(dead_code)] // pure set transition; remove with the host wiring.
pub fn resolving_remove(
    resolving: &BTreeSet<String>,
    cell_key: &str,
) -> BTreeSet<String> {
    let mut next = resolving.clone();
    next.remove(cell_key);
    next
}

/// React's breadcrumb memo: `null` without an origin, otherwise the origin plus
/// the truncated flag and the two `length > 0` gates.
#[allow(dead_code)] // VM mirror; remove with the host wiring.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BreadcrumbVm {
    pub origin: FkOrigin,
    pub truncated: bool,
    pub can_go_back: bool,
    pub can_go_forward: bool,
}

/// Pure projection of the hook's `breadcrumb` useMemo.
#[allow(dead_code)] // pure decision; remove with the host wiring.
pub fn derive_breadcrumb(
    origin: Option<&FkOrigin>,
    truncated: bool,
    back_len: usize,
    forward_len: usize,
) -> Option<BreadcrumbVm> {
    origin.map(|origin| BreadcrumbVm {
        origin: origin.clone(),
        truncated,
        can_go_back: back_len > 0,
        can_go_forward: forward_len > 0,
    })
}

/// React `handleShowReferencingRows`'s decision: only a row that exists can be
/// opened; the host performs `openReferencingRows`.
#[allow(dead_code)] // pure decision; remove with the host wiring.
pub fn resolve_referencing_rows(
    row_index: usize,
    row_count: usize,
) -> Option<usize> {
    (row_index < row_count).then_some(row_index)
}
