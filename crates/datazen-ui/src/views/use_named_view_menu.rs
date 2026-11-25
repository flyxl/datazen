//! Pure logic of `windows/connection/useNamedViewMenu.ts`.
//!
//! The hook decides three things for a table panel: the persistent scope of
//! its saved views, whether the menu is offered at all, and how applying a
//! saved view reaches the table's existing actions. It also derives the dirty
//! flag that warns a save without blocking it. This module keeps those
//! decisions as pure functions over host-owned state — no store, IPC, timer
//! or clipboard write. The DTO types and the filter-readout helper are reused
//! from the already-ported `filters::named_view_menu` component.
//!
//! Two invariants are preserved: the scope is a *persistent* identity (a
//! `dbSessionId` is runtime state and never appears here), and applying a view
//! goes through the three existing actions (`setFilters`/`setSort`/
//! `setVisibleColumns`) or the raw-WHERE channel — never a second write path.
//!
//! ## Removal stage
//!
//! No production host mounts this menu yet; every reusable item carries a
//! per-item `#[allow(dead_code)]` with removal stage "drop when the table host
//! owns the named-view menu".

use serde_json::Value;

use crate::components::filter::{
    read_view_filters, FilterLogic, NamedViewScope, ViewFilter, ViewSort,
};

/// A filter condition as the panel owns it (a view's filter carries a value).
#[allow(dead_code)] // VM mirror; remove when the host owns filter state.
#[derive(Clone, Debug, PartialEq)]
pub struct NamedViewFilterVm {
    pub column: String,
    pub operator: String,
    pub value: Option<Value>,
}

/// A saved view as the apply path reads it (DBR-04 adds the raw predicate).
#[allow(dead_code)] // VM mirror; remove when the host owns saved views.
#[derive(Clone, Debug, PartialEq)]
pub struct SavedViewVm {
    pub filters: Vec<NamedViewFilterVm>,
    pub filter_logic: FilterLogic,
    /// A saved raw WHERE predicate replays through the raw channel.
    pub raw_where: Option<String>,
    pub sorts: Vec<ViewSort>,
    pub visible_columns: Option<Vec<String>>,
}

#[allow(dead_code)] // VM mirror; remove when the host owns saved views.
impl Default for SavedViewVm {
    fn default() -> Self {
        Self {
            filters: Vec::new(),
            filter_logic: FilterLogic::And,
            raw_where: None,
            sorts: Vec::new(),
            visible_columns: None,
        }
    }
}

/// The one action that applies a saved view's filters.
#[allow(dead_code)] // decision; remove when the host wires the menu.
#[derive(Clone, Debug, PartialEq)]
pub enum ApplyAction {
    /// A saved raw predicate (DBR-04) replays through the raw channel.
    ApplyRawWhere(String),
    SetFilters(Vec<NamedViewFilterVm>, FilterLogic),
    SetSort { column: String, descending: bool },
    SetVisibleColumns(Vec<String>),
}

/// How a saved view reaches the table, in action order.
#[allow(dead_code)] // decision; remove when the host wires the menu.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ApplyPlan {
    /// A view naming an unknown operator is refused: nothing is applied.
    pub refused: bool,
    pub actions: Vec<ApplyAction>,
}

/// The hook's `scope`: `null` when there is no persistent connection id.
#[allow(dead_code)] // pure helper; remove when the host owns the scope.
pub fn derive_scope(
    connection_id: Option<&str>,
    database: &str,
    schema: Option<&str>,
    table_name: &str,
) -> Option<NamedViewScope> {
    let connection_id = connection_id?;
    if connection_id.is_empty() {
        return None;
    }
    Some(NamedViewScope {
        connection_id: connection_id.to_owned(),
        database: if database.is_empty() {
            None
        } else {
            Some(database.to_owned())
        },
        schema: schema.map(str::to_owned),
        table: table_name.to_owned(),
    })
}

/// React's two menu guards: no scope, or a backend that cannot serve views.
#[allow(dead_code)] // pure helper; remove when the host wires the menu.
pub fn menu_present(scope: Option<&NamedViewScope>, backend_available: bool) -> bool {
    scope.is_some() && backend_available
}

/// Column/operator part of a view filter, for the shared readout helper.
fn as_view_filter(filter: &NamedViewFilterVm) -> ViewFilter {
    ViewFilter {
        column: filter.column.clone(),
        operator: filter.operator.clone(),
    }
}

/// React `onApply`: raw predicate first, then filters, first sort and column
/// layout. An unknown operator refuses the whole view.
#[allow(dead_code)] // pure helper; remove when the host wires the menu.
pub fn apply_plan(view: &SavedViewVm) -> ApplyPlan {
    let mut plan = ApplyPlan::default();
    let raw = view.raw_where.as_deref().unwrap_or("");
    if !raw.is_empty() {
        plan.actions.push(ApplyAction::ApplyRawWhere(raw.to_owned()));
    } else {
        let readout = read_view_filters(&view.filters.iter().map(as_view_filter).collect::<Vec<_>>());
        if !readout.unknown_operators.is_empty() {
            plan.refused = true;
            return plan;
        }
        plan.actions.push(ApplyAction::SetFilters(view.filters.clone(), view.filter_logic));
    }
    if let Some(sort) = view.sorts.first() {
        plan.actions.push(ApplyAction::SetSort {
            column: sort.column.clone(),
            descending: sort.descending,
        });
    }
    if let Some(columns) = view.visible_columns.as_ref() {
        plan.actions.push(ApplyAction::SetVisibleColumns(columns.clone()));
    }
    plan
}

/// React `filterDraftEqualsApplied`: same logic and the same JSON of filters.
#[allow(dead_code)] // pure helper; remove when the host owns the filter panel.
pub fn filter_draft_equals_applied(
    draft: &[NamedViewFilterVm],
    draft_logic: FilterLogic,
    applied: &[NamedViewFilterVm],
    applied_logic: FilterLogic,
) -> bool {
    draft_logic == applied_logic
        && draft.len() == applied.len()
        && draft.iter().zip(applied).all(|(left, right)| {
            left.column == right.column
                && left.operator == right.operator
                && left.value == right.value
        })
}

/// React's `dirty` flag: a diverging draft warns but does not block a save.
#[allow(dead_code)] // pure helper; remove when the host wires the menu.
pub fn draft_is_dirty(
    draft: &[NamedViewFilterVm],
    draft_logic: FilterLogic,
    applied: &[NamedViewFilterVm],
    applied_logic: FilterLogic,
) -> bool {
    !filter_draft_equals_applied(draft, draft_logic, applied, applied_logic)
}
