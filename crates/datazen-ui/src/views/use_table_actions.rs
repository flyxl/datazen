//! `windows/connection/useTableActions.ts` — the panel-scoped table action
//! table, ported as a pure decision module for T624.
//!
//! React's hook renders no DOM: it is a `useMemo` returning 40 bindings, each
//! calling exactly one store method with the panel id. The decisions worth
//! porting are the ones the bindings take *before* they call the store:
//!
//! - **navigation guard**: an open editor on this panel must commit its draft
//!   first; an unacceptable draft stops the navigation instead of losing it.
//! - **notice invalidation**: page-moving actions drop the "new row not
//!   located" notice; a rollback drops it too; render-only actions and the
//!   unstage of a delete do not.
//! - **count strategy**: the one resolver every read goes through, including
//!   `load` and the exact-count re-read, with the same auto/threshold rules.
//!
//! This module decides those facts over host-owned inputs and emits an
//! [`ActionPlan`]; the host owns the store, the editor and the panel id. No IPC,
//! timer or promise lives here.
//!
//! ## Consumer / removal stage
//!
//! No production table host mounts these decisions yet. Every reusable item
//! carries a per-item `#[allow(dead_code)]` with this stage as its removal note.
//! No locale key, colour or icon is introduced: the React hook has no copy.

/// The 40 bindings of React's `TableActions`, in declaration order.
#[allow(dead_code)] // contract mirror; consumed by the host action table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum TableAction {
    Load,
    Reload,
    CountExact,
    SetDefaultOrderOverride,
    SetSort,
    AddSort,
    RemoveSort,
    ClearSorts,
    SetColumnOrder,
    SetFrozenColumnCount,
    RemoveFilter,
    ClearFilters,
    AddFilter,
    SetFilters,
    UpdateFilter,
    SetFilterLogic,
    ApplyFilters,
    SetFilterMode,
    SetAnyColumnDraft,
    SetRawWhereDraft,
    ApplyAnyColumn,
    ApplyRawWhere,
    SetFilterPanelOpen,
    SetVisibleColumns,
    SetPage,
    SetPageSize,
    StartEdit,
    StageCellChange,
    StagePasteWrites,
    AddDraftRowsFromCopy,
    CancelEdit,
    SelectRow,
    ToggleSelectAll,
    SetGridSelection,
    DeleteRows,
    UnstageRowDelete,
    PreviewPendingChanges,
    CommitPendingChanges,
    RollbackPendingChanges,
    SetDetailRow,
}

impl TableAction {
    #[allow(dead_code)] // contract mirror; consumed by the parity tests.
    pub(crate) const ALL: [Self; 40] = [
        Self::Load,
        Self::Reload,
        Self::CountExact,
        Self::SetDefaultOrderOverride,
        Self::SetSort,
        Self::AddSort,
        Self::RemoveSort,
        Self::ClearSorts,
        Self::SetColumnOrder,
        Self::SetFrozenColumnCount,
        Self::RemoveFilter,
        Self::ClearFilters,
        Self::AddFilter,
        Self::SetFilters,
        Self::UpdateFilter,
        Self::SetFilterLogic,
        Self::ApplyFilters,
        Self::SetFilterMode,
        Self::SetAnyColumnDraft,
        Self::SetRawWhereDraft,
        Self::ApplyAnyColumn,
        Self::ApplyRawWhere,
        Self::SetFilterPanelOpen,
        Self::SetVisibleColumns,
        Self::SetPage,
        Self::SetPageSize,
        Self::StartEdit,
        Self::StageCellChange,
        Self::StagePasteWrites,
        Self::AddDraftRowsFromCopy,
        Self::CancelEdit,
        Self::SelectRow,
        Self::ToggleSelectAll,
        Self::SetGridSelection,
        Self::DeleteRows,
        Self::UnstageRowDelete,
        Self::PreviewPendingChanges,
        Self::CommitPendingChanges,
        Self::RollbackPendingChanges,
        Self::SetDetailRow,
    ];

    /// The store method each binding delegates to. `selectRow` is the one
    /// binding whose name differs from its method (`setRowModeSelection`).
    #[allow(dead_code)] // delegation contract; consumed by the host and tests.
    pub(crate) fn store_method(self) -> &'static str {
        match self {
            Self::Load => "loadTableData",
            Self::Reload | Self::CountExact => "reloadPanel",
            Self::SetDefaultOrderOverride => "setDefaultOrderOverride",
            Self::SetSort => "setSort",
            Self::AddSort => "addSort",
            Self::RemoveSort => "removeSort",
            Self::ClearSorts => "clearSorts",
            Self::SetColumnOrder => "setColumnOrder",
            Self::SetFrozenColumnCount => "setFrozenColumnCount",
            Self::RemoveFilter => "removeFilter",
            Self::ClearFilters => "clearFilters",
            Self::AddFilter => "addFilter",
            Self::SetFilters => "setFilters",
            Self::UpdateFilter => "updateFilter",
            Self::SetFilterLogic => "setFilterLogic",
            Self::ApplyFilters => "applyFilters",
            Self::SetFilterMode => "setFilterMode",
            Self::SetAnyColumnDraft => "setAnyColumnDraft",
            Self::SetRawWhereDraft => "setRawWhereDraft",
            Self::ApplyAnyColumn => "applyAnyColumn",
            Self::ApplyRawWhere => "applyRawWhere",
            Self::SetFilterPanelOpen => "setFilterPanelOpen",
            Self::SetVisibleColumns => "setVisibleColumns",
            Self::SetPage => "setPage",
            Self::SetPageSize => "setPageSize",
            Self::StartEdit => "startEdit",
            Self::StageCellChange => "stageCellChange",
            Self::StagePasteWrites => "stagePasteWrites",
            Self::AddDraftRowsFromCopy => "addDraftRowsFromCopy",
            Self::CancelEdit => "cancelEdit",
            Self::SelectRow => "setRowModeSelection",
            Self::ToggleSelectAll => "toggleSelectAll",
            Self::SetGridSelection => "setGridSelection",
            Self::DeleteRows => "deleteRows",
            Self::UnstageRowDelete => "unstageRowDelete",
            Self::PreviewPendingChanges => "previewPendingChanges",
            Self::CommitPendingChanges => "commitPendingChanges",
            Self::RollbackPendingChanges => "rollbackPendingChanges",
            Self::SetDetailRow => "setDetailRow",
        }
    }

    /// True for the bindings that move the page: they all run the navigation
    /// guard and drop the notice. Rollback is the sole notice-invalidator that
    /// does NOT run the guard.
    #[allow(dead_code)] // pure gate; consumed by plan_action and tests.
    pub(crate) fn is_page_moving(self) -> bool {
        matches!(
            self,
            Self::Load
                | Self::Reload
                | Self::CountExact
                | Self::SetDefaultOrderOverride
                | Self::SetSort
                | Self::AddSort
                | Self::RemoveSort
                | Self::ClearSorts
                | Self::ClearFilters
                | Self::SetFilters
                | Self::ApplyFilters
                | Self::ApplyAnyColumn
                | Self::ApplyRawWhere
                | Self::SetPage
                | Self::SetPageSize
        )
    }
}

// ── Navigation guard ──────────────────────────────────────────────────────

/// React `resolveEditorBeforeNavigation(panelId)`: an editor on another panel
/// is irrelevant; an editor here commits and a commit reason refuses the move.
#[allow(dead_code)] // VM mirror; supplied by the editor host.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct EditorSnapshot {
    pub(crate) panel_id: String,
    pub(crate) open: bool,
    /// `None` means the draft committed cleanly.
    pub(crate) commit_reason: Option<String>,
}

#[allow(dead_code)] // pure gate; consumed by plan_action and tests.
pub(crate) fn may_navigate(editor: Option<&EditorSnapshot>, panel_id: &str) -> bool {
    let Some(editor) = editor else { return true };
    if !editor.open || editor.panel_id != panel_id {
        return true;
    }
    editor.commit_reason.is_none()
}

// ── Count strategy ────────────────────────────────────────────────────────

/// React `RowCountMode` / `CountStrategy` (the union of both contracts).
#[allow(dead_code)] // VM mirror; supplied by settings and the driver.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CountMode {
    None,
    Exact,
    Estimated,
    Auto,
}

#[allow(dead_code)] // VM mirror; supplied by settings and the driver.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CountStrategy {
    Unsupported,
    Exact,
    Estimated,
}

/// React default settings: `auto` mode, 200 000-row estimate threshold.
#[allow(dead_code)] // defaults mirror; consumed by the panel resolver and tests.
pub(crate) const DEFAULT_ROW_COUNT_MODE: CountMode = CountMode::Auto;
#[allow(dead_code)] // defaults mirror; consumed by the panel resolver and tests.
pub(crate) const DEFAULT_ROW_COUNT_ESTIMATE_THRESHOLD: f64 = 200_000.0;

/// React `resolveCountStrategy`: `none`→unsupported, `exact`/`estimated` map
/// directly, `auto` weighs a known finite row count against a positive finite
/// threshold. An unknown size or an invalid threshold yields no opinion.
#[allow(dead_code)] // pure helper; consumed by panel resolution and tests.
pub(crate) fn resolve_count_strategy(
    mode: CountMode,
    threshold: f64,
    known_row_count: Option<f64>,
) -> Option<CountStrategy> {
    match mode {
        CountMode::None => Some(CountStrategy::Unsupported),
        CountMode::Exact => Some(CountStrategy::Exact),
        CountMode::Estimated => Some(CountStrategy::Estimated),
        CountMode::Auto => {
            let known = known_row_count?;
            if !known.is_finite() {
                return None;
            }
            if !threshold.is_finite() || threshold <= 0.0 {
                return Some(CountStrategy::Exact);
            }
            Some(if known > threshold {
                CountStrategy::Estimated
            } else {
                CountStrategy::Exact
            })
        }
    }
}

/// React `resolvePanelCountStrategy`: the settings defaults plus the catalog's
/// known row count (a prior, not a fact) from the panel's load target.
#[allow(dead_code)] // pure helper; consumed by the load plan and tests.
pub(crate) fn resolve_panel_count_strategy(
    mode: CountMode,
    threshold: Option<f64>,
    known_row_count: Option<f64>,
) -> Option<CountStrategy> {
    resolve_count_strategy(mode, threshold.unwrap_or(DEFAULT_ROW_COUNT_ESTIMATE_THRESHOLD), known_row_count)
}

// ── Load target / reload options ──────────────────────────────────────────

/// React `TableChangeContext` — the immutable load target of one panel.
#[allow(dead_code)] // VM mirror; supplied by the panel store.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TableLoadContext {
    pub(crate) connection_id: Option<String>,
    pub(crate) db_session_id: String,
    pub(crate) driver_type: String,
    pub(crate) database: Option<String>,
    pub(crate) schema: Option<String>,
    pub(crate) table: String,
}

/// The object `load` hands to `loadTableData`: panel id, the context verbatim,
/// and the resolved count strategy.
#[allow(dead_code)] // props mirror; consumed by the host load call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LoadCall {
    pub(crate) panel_id: String,
    pub(crate) context: TableLoadContext,
    pub(crate) count_strategy: Option<CountStrategy>,
}

#[allow(dead_code)] // pure helper; remove when the table host wires actions.
pub(crate) fn load_call(panel_id: &str, context: &TableLoadContext, known_row_count: Option<f64>) -> LoadCall {
    LoadCall {
        panel_id: panel_id.to_owned(),
        context: context.clone(),
        count_strategy: resolve_panel_count_strategy(DEFAULT_ROW_COUNT_MODE, None, known_row_count),
    }
}

/// React `reloadPanel(panelId, { skipCount: false, countStrategy: 'exact' })`
/// used by `countExact` — a full re-read, not a paged one.
#[allow(dead_code)] // props mirror; consumed by the countExact plan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ReloadOptions {
    pub(crate) skip_count: bool,
    pub(crate) count_strategy: CountStrategy,
}

#[allow(dead_code)] // pure helper; remove when the table host wires actions.
pub(crate) fn exact_reload_options() -> ReloadOptions {
    ReloadOptions { skip_count: false, count_strategy: CountStrategy::Exact }
}

// ── Action plans ──────────────────────────────────────────────────────────

/// The decision of one binding: whether it may proceed, whether it drops the
/// insert notice, and its special call options.
#[allow(dead_code)] // decision mirror; consumed by the host action table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ActionPlan {
    pub(crate) action: TableAction,
    pub(crate) proceed: bool,
    pub(crate) invalidate_notice: bool,
    pub(crate) exact_reload: bool,
}

/// React's per-binding guards, in one pure decision.
///
/// A page-moving action is refused when the editor cannot commit, and takes
/// its notice invalidation with it. `rollbackPendingChanges` always drops the
/// notice but has no navigation guard. Every other binding is unguarded.
#[allow(dead_code)] // decision; remove when the table host wires actions.
pub(crate) fn plan_action(action: TableAction, may_navigate: bool) -> ActionPlan {
    let (proceed, invalidate_notice) = if action == TableAction::RollbackPendingChanges {
        (true, true)
    } else if action.is_page_moving() {
        (may_navigate, may_navigate)
    } else {
        (true, false)
    };
    ActionPlan {
        action,
        proceed,
        invalidate_notice,
        exact_reload: action == TableAction::CountExact && proceed,
    }
}

/// Plan all 40 bindings under one guard result, in declaration order.
#[allow(dead_code)] // decision; remove when the table host wires actions.
pub(crate) fn plan_all(may_navigate: bool) -> Vec<ActionPlan> {
    TableAction::ALL.iter().map(|action| plan_action(*action, may_navigate)).collect()
}

/// The bindings that drop the insert notice (React `NOTICE_INVALIDATING`).
#[allow(dead_code)] // pure projection; consumed by the parity tests.
pub(crate) fn notice_invalidating() -> Vec<TableAction> {
    TableAction::ALL.iter().copied().filter(|action| plan_action(*action, true).invalidate_notice).collect()
}
