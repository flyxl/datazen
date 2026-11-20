//! The fourth step of the Data Sync wizard — T024.
//!
//! Ported from `packages/frontend/src/windows/data-sync/DataSyncCompareStep.tsx`
//! (147 lines): the comparison result with the table list and the paged row
//! diffs of the selected table. The step renders one of three branches:
//!
//! * `sync_state == 'comparing'` — spinner + `sync.comparing` + cancel;
//! * `compared` — summary, table list, and the selected table's diff detail;
//! * otherwise — the `sync.comparePrompt` empty state.
//!
//! ## Key convention
//!
//! React `data-testid` becomes the element key, verbatim:
//!
//! | part | key |
//! |---|---|
//! | root | `data-sync-compare-step` (internal) |
//! | comparing branch | `data-sync-comparing` (internal) |
//! | comparing spinner | `data-sync-comparing-spinner` |
//! | comparing text | `data-sync-comparing-text` (internal) |
//! | cancel button | `data-sync-cancel` |
//! | compared branch | `data-sync-compared` (internal) |
//! | summary mount (T288 fills it) | `data-sync-compare-summary` |
//! | compared body | `data-sync-compare-body` (internal) |
//! | table-list mount (T383 fills it) | `data-sync-table-list` |
//! | detail column | `data-sync-compare-detail` (internal) |
//! | diff-detail mount (T308 fills it) | `data-sync-diff-detail` |
//! | no-selection prompt | `data-sync-select-table-prompt` (internal) |
//! | no-selection text | `data-sync-select-table-prompt-text` (internal) |
//! | compare prompt branch | `data-sync-compare-prompt` (internal) |
//! | compare prompt text | `data-sync-compare-prompt-text` (internal) |
//!
//! ## Mounting
//!
//! Like `ColumnMappingEditor`'s `data-transfer-source-filter` shell, the
//! three child components are **not ported yet** — `CompareSummary` (T288),
//! `DiffDetail` (T308), `TableListPanel` (T383) — so the compared branch
//! renders empty keyed mount slots for them. The step owns the view model
//! (all of `DataSyncCompareStepProps` as data) and the derivations it
//! computes for the detail child ([`DataSyncCompareStepVm::selected_table_page`]);
//! the child-owned callbacks (`onCopyReport`, `onExplainDiff`,
//! `onFilterChange`, `onSearchChange`, `onSelectTable`, `onUpdateRows`,
//! `onSelectAllOperation`, `onClearAllOperation`, `isOperationSelected`,
//! `onPageChange`) are **not** modelled here — each child task owns its
//! callbacks and adds its builder wiring when it fills its slot. The only
//! callback this slice wires is `onCancel`, the comparing branch's button.
//!
//! [`DataSyncCompareStep::build_inner`] returns the step element without
//! its kit root (no overlay layer: the step mounts no select/menu), so the
//! Data Sync window (T270) can root the whole wizard once; `build` is
//! `build_inner` plus the kit root, exactly as the probes mount it.
//!
//! ## Documented deviations from React
//!
//! * **`source_filter` was unmodelled at T024** (the step never reads it);
//!   T336 (MappingPanel) added the field — the first consumer — mirroring
//!   `DataSyncSourceFilter` as `StructuredSourceFilter`.
//! * **Empty-string `next` cursor counts as no next page.** React's
//!   `Boolean(pageCursors[key]?.next)` is falsy for `""` as well as for
//!   `null`; [`has_next_page`] mirrors that instead of a plain
//!   `is_some()`.
//! * **No `flex-wrap`.** React's summary row wraps; Rune rows do not wrap,
//!   so the (empty, for now) summary slot is a single line. The gate's
//!   960pt minimum still fits the step.

// The production consumer is the Data Sync window (T270); until then the
// component tests are the only callers, so the dead-code lint is silenced
// here rather than in a crate-wide allow.
#![allow(dead_code)]

use std::collections::HashMap;
use std::rc::Rc;

use rune::{col, row, text, El};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use crate::i18n::t;
use crate::ui::kit::kit;
use crate::ui::spinner::{Spinner, SpinnerSize};
use crate::views::data_transfer::source_filter_editor::StructuredSourceFilter;

/// React `SyncState` (`windows/data-sync/utils.ts:18`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum SyncState {
    #[default]
    Idle,
    Inspecting,
    Comparing,
    Compared,
    Executing,
    Unknown,
    Done,
}

/// React `DataSyncOperation` (`backend-client/types/dataSync.ts:33`):
/// the row operation of one comparison change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DataSyncOperation {
    Insert,
    Update,
    Delete,
    Unchanged,
}

/// React `RowOperation` (`DataSyncCompareStep.tsx:16`):
/// `Exclude<DataSyncRowChange['operation'], 'UNCHANGED'>` — the
/// operations a review selection can scope.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RowOperation {
    Insert,
    Update,
    Delete,
}

/// React `DataSyncRowChange` (`backend-client/types/dataSync.ts:217-224`).
///
/// `Value` is `serde_json::Value` (the frontend `Value` is
/// `string | number | boolean | null | Record<string, unknown> | unknown[]`),
/// so a `null` cell rides as `Value::Null` inside the row vector.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DataSyncRowChange {
    pub operation: DataSyncOperation,
    pub key: Vec<serde_json::Value>,
    pub source_row: Option<Vec<serde_json::Value>>,
    pub target_row: Option<Vec<serde_json::Value>>,
    pub changed_columns: Vec<String>,
    pub selected: bool,
}

/// React `DataSyncMappingStatus` (`backend-client/types/dataSync.ts:36-41`):
/// the table-pair classification of one inspection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum DataSyncMappingStatus {
    #[default]
    Matched,
    UnmappedSource,
    UnmappedTarget,
    Disabled,
    Incompatible,
}

/// React `TableResult` (`backend-client/types/dataSync.ts:233-261`),
/// aliased `DataSyncTableResult` in `commands/sync.ts:59`.
///
/// The interface deliberately spans two projections — the inspect result
/// (columns/rows, no row counts) and the frozen preview summary (counts
/// and paging, never row payloads) — so the count/paging fields are
/// `Option`: absent on the face that does not carry them.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct DataSyncTableResult {
    pub source_table: String,
    pub target_table: String,
    pub status: DataSyncMappingStatus,
    pub incompatible_reason: Option<String>,
    pub warnings: Vec<String>,
    pub columns: Vec<String>,
    pub column_types: Vec<String>,
    pub primary_keys: Vec<String>,
    pub unchanged_count: Option<usize>,
    pub insert_count: Option<usize>,
    pub update_count: Option<usize>,
    pub delete_count: Option<usize>,
    pub row_count: Option<usize>,
    pub page_size: Option<usize>,
    pub first_cursor: Option<String>,
    pub has_more: Option<bool>,
    pub rows: Vec<DataSyncRowChange>,
    /// React `sourceFilter?: DataSyncSourceFilter` — the structured filter
    /// this table's predicate/recordset editors edit. T024 left it
    /// unmodelled (the compare step never reads it); T336 (MappingPanel)
    /// is the first consumer, so it lands here. The mirror is
    /// `StructuredSourceFilter` (`views/data_transfer/source_filter_editor.rs`,
    /// T202) — the `{filters, logic, recordset}` shape.
    pub source_filter: Option<StructuredSourceFilter>,
}

/// React `TableDiffFilter` (`windows/data-sync/mappingView.ts:16`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum TableDiffFilter {
    #[default]
    All,
    Insert,
    Update,
    Delete,
    Unchanged,
    Incompatible,
}

/// React `CompareSummaryStats` (`windows/data-sync/mappingView.ts:95-101`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct CompareSummaryStats {
    pub inserts: usize,
    pub updates: usize,
    pub deletes: usize,
    pub unchanged_tables: usize,
    pub incompatible: usize,
}

/// React `DataSyncPageCursor`
/// (`windows/data-sync/useDataSyncSelectionState.ts:5-9`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct DataSyncPageCursor {
    pub current: Option<String>,
    pub next: Option<String>,
    pub previous: Vec<String>,
}

/// React `DataSyncSelectionMode` (`backend-client/types/dataSync.ts:283`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DataSyncSelectionMode {
    All,
    Defaults,
}

/// React `DataSyncSelectionExclusion`
/// (`backend-client/types/dataSync.ts:277-280`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DataSyncSelectionExclusion {
    pub operation: RowOperation,
    pub key: Vec<serde_json::Value>,
}

/// React `DataSyncTableSelection` (`backend-client/types/dataSync.ts:286-292`):
/// the server-owned selection of comparison rows for one table and
/// operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DataSyncTableSelection {
    pub source_table: String,
    pub target_table: String,
    pub selection_mode: DataSyncSelectionMode,
    pub operations: Vec<RowOperation>,
    pub excluded_rows: Vec<DataSyncSelectionExclusion>,
}

/// React `SyncOptions` (`backend-client/types/dataSync.ts:136-144`).
///
/// The optional fields are `Option` because the wire leaves them absent
/// when the caller does not set them; [`SyncOptions::default`] is
/// `DEFAULT_SYNC_OPTIONS` (`commands/sync.ts:88-96`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SyncOptions {
    pub insert: bool,
    pub update: bool,
    pub delete: bool,
    pub matching_strategy: Option<SyncMatchingStrategy>,
    pub batch_size: Option<u32>,
    pub large_value_mode: Option<SyncLargeValueMode>,
    pub conflict_policy: Option<SyncConflictPolicy>,
}

impl Default for SyncOptions {
    fn default() -> Self {
        Self {
            insert: true,
            update: true,
            delete: false,
            matching_strategy: Some(SyncMatchingStrategy::PrimaryKey),
            batch_size: Some(1000),
            large_value_mode: Some(SyncLargeValueMode::Full),
            conflict_policy: Some(SyncConflictPolicy::Abort),
        }
    }
}

/// React `SyncOptions['matchingStrategy']`: only `'primaryKey'` exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum SyncMatchingStrategy {
    #[default]
    PrimaryKey,
}

/// React `SyncOptions['largeValueMode']`: `'full' | 'hash'`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum SyncLargeValueMode {
    #[default]
    Full,
    Hash,
}

/// React `SyncOptions['conflictPolicy']`: `'abort' | 'skip' | 'force'`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum SyncConflictPolicy {
    #[default]
    Abort,
    Skip,
    Force,
}

/// React `tablePageKey` (`windows/data-sync/mappingView.ts:74-79`):
/// the composite key of a table pair, used to address the paged
/// comparison cursors of one table. The separator is NUL, so no table
/// name can collide with another pair's key.
pub(crate) fn table_page_key(source_table: &str, target_table: &str) -> String {
    format!("{source_table}\u{0}{target_table}")
}

/// The `hasTableSelection` inline in `DataSyncCompareStep.tsx`: a table
/// has a review selection when some scope addresses the same
/// source/target pair.
pub(crate) fn has_table_selection(
    selections: &[DataSyncTableSelection],
    table: &DataSyncTableResult,
) -> bool {
    selections.iter().any(|scope| {
        scope.source_table == table.source_table && scope.target_table == table.target_table
    })
}

/// React `pageIndex[tablePageKey(selectedTable)] ?? 0`.
pub(crate) fn page_index_for(
    page_index: &HashMap<String, usize>,
    table: &DataSyncTableResult,
) -> usize {
    page_index
        .get(&table_page_key(&table.source_table, &table.target_table))
        .copied()
        .unwrap_or(0)
}

/// React `hasPreviousPage`: `Boolean(pageCursors[key]?.previous.length)`.
pub(crate) fn has_previous_page(
    page_cursors: &HashMap<String, DataSyncPageCursor>,
    table: &DataSyncTableResult,
) -> bool {
    page_cursors
        .get(&table_page_key(&table.source_table, &table.target_table))
        .is_some_and(|cursor| !cursor.previous.is_empty())
}

/// React `hasNextPage`: `Boolean(pageCursors[key]?.next)` — falsy for an
/// empty-string cursor as well as for `null`/missing, so the check is
/// "a non-empty next cursor exists", not a plain `is_some()`.
pub(crate) fn has_next_page(
    page_cursors: &HashMap<String, DataSyncPageCursor>,
    table: &DataSyncTableResult,
) -> bool {
    page_cursors
        .get(&table_page_key(&table.source_table, &table.target_table))
        .is_some_and(|cursor| cursor.next.as_deref().is_some_and(|next| !next.is_empty()))
}

/// Which of the step's three branches renders.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CompareStepBranch {
    Comparing,
    Compared,
    Prompt,
}

/// The `DiffDetail` props the step derives for its selected table
/// (`DataSyncCompareStep.tsx`: `hasTableSelection`, `pageIndex`,
/// `hasPreviousPage`, `hasNextPage`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SelectedTablePage {
    pub has_table_selection: bool,
    pub page_index: usize,
    pub has_previous_page: bool,
    pub has_next_page: bool,
}

/// The controlled state of the step. Fields mirror
/// `DataSyncCompareStepProps` one to one; the ones only the child
/// components consume (`rows`, `stats`, `filter`, `search`,
/// `selected_table_key`, `options`, `explain_loading`) are carried for
/// T288/T308/T383, which read them when they fill their slots.
#[derive(Clone, Debug, Default)]
pub(crate) struct DataSyncCompareStepVm {
    pub sync_state: SyncState,
    pub compared: bool,
    pub rows: Vec<DataSyncTableResult>,
    pub stats: CompareSummaryStats,
    pub filter: TableDiffFilter,
    pub search: String,
    pub selected_table_key: Option<String>,
    pub selected_table: Option<DataSyncTableResult>,
    pub options: SyncOptions,
    pub table_selections: Vec<DataSyncTableSelection>,
    pub page_loading: bool,
    pub page_index: HashMap<String, usize>,
    pub page_cursors: HashMap<String, DataSyncPageCursor>,
    pub explain_loading: bool,
}

impl DataSyncCompareStepVm {
    /// React's branch order: `syncState === 'comparing'` wins over
    /// `compared`, and `compared` wins over the prompt.
    pub(crate) fn branch(&self) -> CompareStepBranch {
        if self.sync_state == SyncState::Comparing {
            CompareStepBranch::Comparing
        } else if self.compared {
            CompareStepBranch::Compared
        } else {
            CompareStepBranch::Prompt
        }
    }

    /// The derived `DiffDetail` props for the selected table, or `None`
    /// when no table is selected.
    pub(crate) fn selected_table_page(&self) -> Option<SelectedTablePage> {
        let table = self.selected_table.as_ref()?;
        Some(SelectedTablePage {
            has_table_selection: has_table_selection(&self.table_selections, table),
            page_index: page_index_for(&self.page_index, table),
            has_previous_page: has_previous_page(&self.page_cursors, table),
            has_next_page: has_next_page(&self.page_cursors, table),
        })
    }
}

/// Builder for the compare step. `S` is the host state that owns the
/// cancel action.
pub(crate) struct DataSyncCompareStep<S: 'static> {
    vm: DataSyncCompareStepVm,
    dark: bool,
    on_cancel: Option<Rc<dyn Fn(&mut S)>>,
}

impl<S: 'static> DataSyncCompareStep<S> {
    pub(crate) fn new(vm: DataSyncCompareStepVm) -> Self {
        Self {
            vm,
            dark: true,
            on_cancel: None,
        }
    }

    /// React `onCancel`: the comparing branch's cancel button.
    pub(crate) fn on_cancel(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_cancel = Some(Rc::new(f));
        self
    }

    /// The compare step on its own, exactly as the probes mount it.
    pub(crate) fn build(self, dark: bool) -> El {
        let (root, layer) = self.build_inner(dark);
        rune_kit::root(root, layer)
    }

    /// The same step without its kit root: the step element plus the
    /// overlay layer (empty — the step mounts no select/menu). The Data
    /// Sync window (T270) calls this once and roots the whole wizard
    /// once.
    pub(crate) fn build_inner(mut self, dark: bool) -> (El, Vec<El>) {
        self.dark = dark;
        let k = kit(dark);
        // React root: `flex min-h-0 flex-1 flex-col overflow-hidden`.
        let mut root = col()
            .key("data-sync-compare-step")
            .w_full()
            .grow(1.0)
            .min_h(0.0);
        root = match self.vm.branch() {
            CompareStepBranch::Comparing => root.child(self.comparing_branch(k)),
            CompareStepBranch::Compared => root.child(self.compared_branch(k)),
            CompareStepBranch::Prompt => root.child(self.prompt_branch(k)),
        };
        (root, Vec::new())
    }

    /// React's `syncState === 'comparing'` branch: spinner + comparing
    /// text + ghost cancel button, centered.
    fn comparing_branch(&self, k: &Kit) -> El {
        let cancel = self
            .on_cancel
            .clone()
            .unwrap_or_else(|| Rc::new(|_: &mut S| {}));
        let mut button = Button::new(t("common.cancel"))
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Small);
        button = button.on_press(move |s: &mut S| cancel(s));
        row()
            .key("data-sync-comparing")
            .grow(1.0)
            .center()
            .gap(8.0)
            .child(
                Spinner::new("data-sync-comparing-spinner")
                    .size(SpinnerSize::Lg)
                    .build(self.dark),
            )
            .child(
                text(t("sync.comparing"))
                    .key("data-sync-comparing-text")
                    .font(k.ty.text_small)
                    .color(k.c.text_muted),
            )
            .child(button.build(k).key("data-sync-cancel"))
    }

    /// React's `compared` branch: the summary mount, then the bordered
    /// body with the table-list mount and the detail column.
    fn compared_branch(&self, k: &Kit) -> El {
        let mut branch = col()
            .key("data-sync-compared")
            .w_full()
            .grow(1.0)
            .min_h(0.0);
        // `CompareSummary` (T288) fills this mount.
        branch = branch.child(col().key("data-sync-compare-summary").w_full().min_w(0.0));
        // React body: `flex min-h-0 flex-1 rounded-lg border border-edge`.
        let mut body = row()
            .key("data-sync-compare-body")
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .border(1.0, k.c.border_base)
            .radius(8.0);
        // `TableListPanel` (T383) fills this mount; its React root is
        // `flex-1`, the detail column `flex-[1.4]`.
        body = body.child(
            col()
                .key("data-sync-table-list")
                .grow(1.0)
                .min_w(0.0)
                .min_h(0.0),
        );
        body = body.child(self.detail_column(k));
        branch = branch.child(body);
        branch
    }

    /// The detail column: `flex min-h-0 min-w-0 flex-[1.4] flex-col`.
    /// With a selected table the `DiffDetail` mount (T308) fills it;
    /// without one the select-table prompt renders.
    fn detail_column(&self, k: &Kit) -> El {
        let mut column = col()
            .key("data-sync-compare-detail")
            .grow(1.4)
            .min_w(0.0)
            .min_h(0.0);
        if self.vm.selected_table.is_some() {
            // `DiffDetail` (T308) fills this mount.
            column = column.child(
                col()
                    .key("data-sync-diff-detail")
                    .w_full()
                    .grow(1.0)
                    .min_h(0.0),
            );
        } else {
            column = column.child(
                row()
                    .key("data-sync-select-table-prompt")
                    .grow(1.0)
                    .center()
                    .child(
                        text(t("sync.selectTableForDetail"))
                            .key("data-sync-select-table-prompt-text")
                            .font(k.ty.text_small)
                            .color(k.c.text_muted),
                    ),
            );
        }
        column
    }

    /// React's fallback branch: the compare prompt, centered.
    fn prompt_branch(&self, k: &Kit) -> El {
        row()
            .key("data-sync-compare-prompt")
            .grow(1.0)
            .center()
            .child(
                text(t("sync.comparePrompt"))
                    .key("data-sync-compare-prompt-text")
                    .font(k.ty.text_small)
                    .color(k.c.text_muted),
            )
    }
}
