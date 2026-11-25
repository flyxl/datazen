//! `ResultTableView`, ported 1:1 from
//! `packages/frontend/src/windows/connection/result-workspace/ResultTableView.tsx`.
//!
//! React is the query-result grid: a status bar, the editable/staging pipeline
//! (double-click → stage → preview/commit/rollback), a find bar with filtered
//! rows, mutation executions rendered as an `ExecutionSummaryCard`, and the
//! `DataTable` adapter carrying headers, row detail, export identity and
//! presentation. The Rune port keeps every decision as a pure helper and emits
//! [`ResultTableAction`]s through an injected host port — no fake IPC, promise
//! or timer. The CodeMirror/Pro find and plan dialogs are separate ports; their
//! keyed regions are honest stand-ins the host fills.
//!
//! ## Removal stage
//!
//! No production host mounts this grid yet. The result-workspace host will own
//! the VM and translate the actions into its commands; until then every
//! reusable item carries a per-item `#[allow(dead_code)]` with this removal stage.

use std::rc::Rc;
use rune_kit::SortDir;

use rune::{col, row, text, El};

use crate::i18n::t;
use crate::ui::kit::kit;
use crate::ui::table::{CellValue, Column, DataTable, Row};

mod cells;
mod header;

pub use cells::{filter_rows, FindQuery, GridValue};
#[allow(unused_imports)] // find/replace seam; consumer: the result-workspace host + tests; drop when the find port lands.
pub use cells::{plan_replacements, replace_text, FindOperator};
use header::{data_marker, error_banner, find_bar, notice, pending_bar, read_only_banner, status_bar};

/// React `StatementResult` column metadata.
#[allow(dead_code)] // VM mirror; supplied by the host result feed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResultColumn {
    pub name: String,
    pub data_type: String,
}

/// One staged cell edit (`PendingEdit` in React).
#[allow(dead_code)] // VM mirror; consumed by the staging pipeline and tests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingEdit {
    pub row: usize,
    pub column: String,
    pub original: GridValue,
    pub value: GridValue,
}

/// One catalog identity/editable column from the server verdict.
#[allow(dead_code)] // VM mirror; supplied by the verdict feed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EditColumn {
    pub column: String,
    pub source_column: String,
}

/// React `ServerEditability` (the table-level verdict, never recomputed here).
#[allow(dead_code)] // VM mirror; supplied by the verdict feed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ServerEditability {
    pub writable: bool,
    pub reason: Option<String>,
    pub identity_columns: Vec<EditColumn>,
    pub editable_columns: Vec<EditColumn>,
}

/// Host-owned projection of the component props and the store settings it reads.
#[allow(dead_code)] // VM surface; owned by the result-workspace host.
#[derive(Clone, Debug, Default)]
pub struct ResultTableVm {
    pub sql: String,
    pub columns: Vec<ResultColumn>,
    pub rows: Vec<Vec<GridValue>>,
    pub execution_time_ms: i64,
    pub rows_affected: Option<i64>,
    pub truncated: bool,
    pub query_result_limit: usize,
    pub source_writable_mapping_verified: bool,
    pub has_runtime_binding: bool,
    pub server_verdict: Option<ServerEditability>,
    pub safe_mode: bool,
    pub can_commit: bool,
    pub row_detail_index: Option<usize>,
    pub find_open: bool,
    pub find_query: FindQuery,
    pub find_replacement: String,
    pub pending_edits: Vec<PendingEdit>,
    pub editing_cell: Option<(usize, String)>,
    pub commit_notice: Option<String>,
    pub edit_error: Option<(String, Option<String>)>,
    pub row_refusal: Option<String>,
    pub preview_open: bool,
    pub sort: Option<(usize, SortDir)>,
}

/// Every host command this view requests.
#[allow(dead_code)] // host port; removed with the result-workspace wiring.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResultTableAction {
    ToggleFind,
    CloseFind,
    SelectRow(usize),
    OpenCell { row: usize, column: String },
    EditCell { row: usize, column: String, value: GridValue },
    StageEdit(PendingEdit),
    PreviewEdits,
    CommitEdits,
    RollbackEdits,
    ToggleSort(usize),
}

pub type ActionFn<S> = Rc<dyn Fn(&mut S, ResultTableAction) + 'static>;

pub fn emit<S: 'static>(
    on_action: &Option<ActionFn<S>>,
    action: ResultTableAction,
) -> impl Fn(&mut S) + Clone {
    let callback = on_action.clone();
    move |s: &mut S| {
        if let Some(callback) = &callback {
            callback(s, action.clone());
        }
    }
}

/// React `isMutationExecution`: no tabular output + a mutating top-level verb.
#[allow(dead_code)] // pure branch; consumed by build and tests.
pub fn is_mutation_execution(vm: &ResultTableVm) -> bool {
    if !vm.columns.is_empty() || !vm.rows.is_empty() {
        return false;
    }
    let verb = vm.sql.split_whitespace().next().map_or_else(String::new, |word| {
        word.trim_matches(|c: char| !c.is_alphabetic()).to_ascii_uppercase()
    });
    matches!(verb.as_str(), "INSERT" | "UPDATE" | "DELETE" | "CREATE" | "ALTER" | "DROP" | "TRUNCATE")
}

/// React `resolveResultWriteBack` with the origin release channel closed.
#[allow(dead_code)] // pure helper; consumed by can_edit and tests.
pub fn result_writable(vm: &ResultTableVm) -> bool {
    let server = vm.server_verdict.clone().unwrap_or_default();
    if !server.writable && server.reason.as_deref() == Some("sessionReadOnly") {
        return false;
    }
    vm.source_writable_mapping_verified && vm.has_runtime_binding
}

/// React `canEdit = !safeMode && !!onCommitResultCells && resultWritable`.
#[allow(dead_code)] // pure helper; consumed by build and tests.
pub fn can_edit(vm: &ResultTableVm) -> bool {
    !vm.safe_mode && vm.can_commit && result_writable(vm)
}

/// React's read-only reason: `null` when the result may be written back.
#[allow(dead_code)] // pure helper; consumed by build and tests.
pub fn read_only_reason(vm: &ResultTableVm) -> Option<String> {
    if result_writable(vm) {
        None
    } else {
        Some(
            vm.server_verdict
                .as_ref()
                .and_then(|verdict| verdict.reason.clone())
                .unwrap_or_else(|| "unknownOrigin".to_owned()),
        )
    }
}

/// React `gridReadOnlyMessageKey`.
#[allow(dead_code)] // pure helper; consumed by the banner and tests.
pub fn grid_read_only_message_key(reason: &str) -> String {
    format!("query.gridReadOnly.{reason}")
}

fn column_names(vm: &ResultTableVm) -> Vec<String> {
    vm.columns.iter().map(|column| column.name.clone()).collect()
}

/// React `effectiveRows` with pending edits applied in place.
#[allow(dead_code)] // pure helper; consumed by view_rows and tests.
pub fn effective_rows(vm: &ResultTableVm) -> Vec<Vec<GridValue>> {
    let mut rows = vm.result_rows();
    let names = column_names(vm);
    for edit in &vm.pending_edits {
        if let Some(index) = names.iter().position(|name| name == &edit.column) {
            if let Some(row) = rows.get_mut(edit.row) {
                if let Some(cell) = row.get_mut(index) {
                    *cell = edit.value.clone();
                }
            }
        }
    }
    rows
}

impl ResultTableVm {
    fn result_rows(&self) -> Vec<Vec<GridValue>> {
        self.rows.clone()
    }
}

/// React `viewRows` / `toOriginal` after find filtering.
#[allow(dead_code)] // pure helper; consumed by build and tests.
pub fn view_rows(vm: &ResultTableVm) -> (Vec<Vec<GridValue>>, Vec<usize>) {
    let names = column_names(vm);
    let effective = effective_rows(vm);
    if vm.find_open && !vm.find_query.needle.is_empty() {
        let indexes = filter_rows(&effective, &names, &vm.find_query);
        let rows = indexes.iter().filter_map(|index| effective.get(*index).cloned()).collect();
        (rows, indexes)
    } else {
        let count = effective.len();
        (effective, (0..count).collect())
    }
}

/// Build the result grid.
#[allow(dead_code)] // consumer: the result-workspace host (not landed).
pub fn build<S: 'static>(
    vm: &ResultTableVm,
    on_action: Option<ActionFn<S>>,
    dark: bool,
) -> El {
    let k = kit(dark);
    let mut root = col().key("result-workspace-table").w_full().h_full().min_h(0.0);

    if is_mutation_execution(vm) {
        return root
            .child(
                row()
                    .key("execution-summary-card")
                    .w_full()
                    .child(text(t("query.executionSuccess")).color(k.c.success.fg)),
            )
            .child(status_bar::<S>(vm, &on_action, dark));
    }

    if !vm.pending_edits.is_empty() {
        root = root.child(pending_bar::<S>(vm, &on_action, dark));
    }
    if vm.preview_open {
        root = root.child(
            row()
                .key("pending-plan-dialog")
                .label(t("resultFind.open"))
                .w_full()
                .min_h(80.0),
        );
    }
    let (rows, originals) = view_rows(vm);
    if vm.find_open {
        root = root.child(find_bar::<S>(vm, &on_action, originals.len(), dark));
    }
    root = root.child(status_bar::<S>(vm, &on_action, dark));

    let mut scroll = col()
        .key("result-grid-scroll")
        .w_full()
        .min_w(0.0)
        .grow(1.0)
        .min_h(0.0);

    if let Some(reason) = read_only_reason(vm) {
        let message = if vm.source_writable_mapping_verified && !vm.has_runtime_binding {
            t("query.resultEditRequiresRefresh").to_owned()
        } else {
            t(&grid_read_only_message_key(&reason)).to_owned()
        };
        scroll = scroll
            .child(data_marker("dt-grid-readonly-reason", &reason))
            .child(read_only_banner(message, dark));
    }
    if let Some(reason) = vm.row_refusal.as_deref() {
        scroll = scroll
            .child(data_marker("dt-grid-row-refusal", reason))
            .child(read_only_banner(t(&grid_read_only_message_key(reason)).to_owned(), dark));
    }
    if let Some(message) = vm.commit_notice.as_deref() {
        scroll = scroll.child(notice(message.to_owned(), dark));
    }
    if let Some((message, hint)) = vm.edit_error.as_ref() {
        scroll = scroll.child(error_banner(message.clone(), hint.clone(), dark));
    }

    let columns: Vec<Column> = vm
        .columns
        .iter()
        .map(|column| {
            let numeric = column.data_type.to_ascii_lowercase().contains("int")
                || column.data_type.to_ascii_lowercase().contains("float")
                || column.data_type.to_ascii_lowercase().contains("numeric");
            if numeric {
                Column::new(column.name.clone()).width(72.0).end()
            } else {
                Column::new(column.name.clone()).width(120.0)
            }
        })
        .collect();
    let table_rows: Vec<Row> = rows
        .iter()
        .enumerate()
        .map(|(index, values)| {
            let cells: Vec<CellValue> = values
                .iter()
                .map(|value| {
                    if value.is_null() {
                        CellValue::new("NULL").muted()
                    } else {
                        CellValue::new(value.display()).mono()
                    }
                })
                .collect();
            Row {
                key: Some(format!("result-row-{}", originals.get(index).copied().unwrap_or(index))),
                cells,
            }
        })
        .collect();
    let mut table = DataTable::new("result-grid", &columns)
        .rows(table_rows)
        .grow()
        .label(t("common.rows"))
        .empty_text(t("sqlFile.noResults"));
    if let Some(sort) = vm.sort {
        table = table.sort(sort.0, sort.1);
    }
    let highlighted = vm
        .row_detail_index
        .and_then(|original| originals.iter().position(|index| *index == original));
    table = table.selected(highlighted);
    if let Some(callback) = on_action.clone() {
        table = table.on_select(move |state: &mut S, row| {
            let original = originals.get(row).copied().unwrap_or(row);
            callback(state, ResultTableAction::SelectRow(original));
        });
    }
    let mut sort_strip = row().key("result-sort-strip").w_full().gap(6.0).flex_wrap().center_y();
    for (index, column) in vm.columns.iter().enumerate() {
        sort_strip = sort_strip.child(
            text(column.name.clone())
                .key(format!("result-sort-{index}"))
                .role("button")
                .focusable(true)
                .color(k.c.text_muted)
                .on_press(emit(&on_action, ResultTableAction::ToggleSort(index))),
        );
    }
    scroll = scroll.child(sort_strip).child(table.build(dark));

    if vm.rows.is_empty() {
        scroll = scroll.child(
            row()
                .key("result-empty-status")
                .w_full()
                .center()
                .child(text(t("sqlFile.noResults")).color(k.c.text_muted)),
        );
    }
    if vm.find_open && !vm.find_query.needle.is_empty() && rows.is_empty() && !vm.rows.is_empty() {
        scroll = scroll.child(
            row()
                .key("result-find-no-matches")
                .w_full()
                .center()
                .child(text(t("resultFind.noRows")).color(k.c.text_muted)),
        );
    }
    root.child(scroll)
}
