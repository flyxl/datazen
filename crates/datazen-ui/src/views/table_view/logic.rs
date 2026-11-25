//! Pure decisions of `TableView`, grouped by the eight React test files:
//! grid columns, editors, insert drafts, dirty index, filter operators,
//! navigation guard and Quick Look. No store, IPC or timer access.

use super::*;
use crate::i18n::t;

/// React `SHARED_FILTER_OPERATORS` (existing set plus the newer operators).
#[allow(dead_code)] // pure table; consumed by the operator resolver and tests.
pub const SHARED_OPERATORS: &[&str] = &[
    "eq", "ne", "gt", "lt", "gte", "lte", "like", "in", "isNull", "isNotNull",
    "notIn", "between", "notBetween", "contains", "notContains", "startsWith", "endsWith",
    "isEmpty", "isNotEmpty",
];

/// React `supportedOperatorsForMeta`: no declaration means the shared set.
#[allow(dead_code)] // pure helper; consumed by the filter panel and tests.
pub fn filter_operators(declared: &[String]) -> Vec<String> {
    if declared.is_empty() {
        SHARED_OPERATORS.iter().map(|op| (*op).to_owned()).collect()
    } else {
        declared.to_vec()
    }
}

/// React `displayedColumnNames`: visibility filter plus explicit order.
#[allow(dead_code)] // pure helper; consumed by the grid and tests.
pub fn displayed_columns(
    columns: &[ColumnMeta],
    visible: Option<&[String]>,
    order: Option<&[String]>,
) -> Vec<String> {
    let names: Vec<String> = columns.iter().map(|column| column.name.clone()).collect();
    if visible.is_none() && order.is_none() {
        return names;
    }
    let visible_set: Vec<String> = visible.map(|list| list.to_vec()).unwrap_or_else(|| names.clone());
    let schema_order: Vec<String> = names.into_iter().filter(|name| visible_set.contains(name)).collect();
    let Some(order) = order else {
        return schema_order;
    };
    let mut ordered = Vec::new();
    for name in order {
        if visible_set.contains(name) && !ordered.contains(name) {
            ordered.push(name.clone());
        }
    }
    for name in schema_order {
        if !ordered.contains(&name) {
            ordered.push(name);
        }
    }
    ordered
}

/// React `hasPendingWork`: staged changes or insert drafts.
#[allow(dead_code)] // pure helper; consumed by the pending bar and tests.
pub fn has_pending_work(vm: &TableViewVm) -> bool {
    !vm.dirty.is_empty() || !vm.drafts.is_empty()
}

/// React `hasPendingWork` counted as cells (one per dirty cell).
#[allow(dead_code)] // pure helper; consumed by the pending bar and tests.
pub fn dirty_counts(vm: &TableViewVm) -> (usize, usize) {
    let cells = vm.dirty.values().map(|row| row.cells.len()).sum();
    (cells, vm.dirty.len())
}

/// React `selectDirtyRow`: a delete-marked row is still a dirty row.
#[allow(dead_code)] // pure helper; consumed by editor gating and tests.
pub fn dirty_row(vm: &TableViewVm, row: usize) -> Option<&DirtyRow> {
    vm.dirty.get(&row)
}

/// React `readOnlyReasonMessageKey`.
#[allow(dead_code)] // pure helper; consumed by the read-only tip and tests.
pub fn read_only_message_key(reason: ReadOnlyReason) -> &'static str {
    match reason {
        ReadOnlyReason::ConnectionReadOnly => "cellEditor.readOnly.connection",
        ReadOnlyReason::DriverCapability => "cellEditor.readOnly.driverCapability",
        ReadOnlyReason::View => "cellEditor.readOnly.view",
        ReadOnlyReason::NoPrimaryKey => "cellEditor.readOnly.noPrimaryKey",
        ReadOnlyReason::AmbiguousRowIdentity => "cellEditor.readOnly.ambiguousRowIdentity",
        ReadOnlyReason::AutoIncrement => "cellEditor.readOnly.autoIncrement",
        ReadOnlyReason::Generated => "cellEditor.readOnly.generated",
        ReadOnlyReason::BinaryColumn => "cellEditor.readOnly.binaryColumn",
        ReadOnlyReason::LargeValue => "cellEditor.readOnly.largeValue",
        ReadOnlyReason::TypeNotEditable => "cellEditor.readOnly.typeNotEditable",
    }
}

/// The reason a double-click refuses to start editing, or `None` when it may.
#[allow(dead_code)] // pure gate; consumed by the editor and tests.
pub fn edit_refusal(vm: &TableViewVm, row: usize) -> Option<ReadOnlyReason> {
    if vm.read_only {
        return Some(vm.read_only_reason.unwrap_or(ReadOnlyReason::ConnectionReadOnly));
    }
    if vm.read_only_reason == Some(ReadOnlyReason::View) {
        return Some(ReadOnlyReason::View);
    }
    if !vm.has_primary_key {
        return Some(ReadOnlyReason::NoPrimaryKey);
    }
    if dirty_row(vm, row).is_some_and(|entry| entry.deleted) {
        return None; // a deleted row blocks the edit but is not a read-only reason
    }
    None
}

/// Whether a double-click may open an editor. Deleted rows and refusals cannot.
#[allow(dead_code)] // pure gate; consumed by the editor and tests.
pub fn can_start_edit(vm: &TableViewVm, row: usize) -> bool {
    edit_refusal(vm, row).is_none() && !dirty_row(vm, row).is_some_and(|entry| entry.deleted)
}

/// React `resolveDefaultTableSort`: panel override beats the global setting.
#[allow(dead_code)] // pure helper; consumed by the grid and tests.
pub fn resolve_default_sort(override_value: &str, setting: &str) -> String {
    if override_value == "inherit" {
        setting.to_owned()
    } else {
        override_value.to_owned()
    }
}

/// React `normalizeSelectionRange`: always `[start, end]` with start <= end.
#[allow(dead_code)] // pure helper; consumed by selection and tests.
pub fn normalize_selection_range(anchor: usize, focus: usize) -> (usize, usize) {
    if anchor <= focus {
        (anchor, focus)
    } else {
        (focus, anchor)
    }
}

/// React `isQuickLookableValue`: blob, JSON value or a string over 120 chars.
#[allow(dead_code)] // pure helper; consumed by Quick Look and tests.
pub fn is_quick_lookable(value: &CellValue, column_type: &str) -> bool {
    let kind = column_type.to_ascii_lowercase();
    if kind.contains("blob")
        || kind.contains("binary")
        || kind.contains("bytea")
        || kind.contains("image")
        || kind.contains("raw")
    {
        return true;
    }
    match value {
        CellValue::Null => false,
        CellValue::Json(_) => true,
        CellValue::Text(text) => text.encode_utf16().count() > 120,
        CellValue::Number(_) | CellValue::Bool(_) | CellValue::Blob(_) => false,
    }
}

/// The Quick Look target from a focused cell, or `None` when nothing is focused.
#[allow(dead_code)] // pure helper; consumed by the keyboard/middle-click handlers and tests.
pub fn quick_look_target(vm: &TableViewVm, row: usize, column: &str) -> Option<(usize, String)> {
    let index = vm.columns.iter().position(|item| item.name == column)?;
    let value = vm.rows.get(row).and_then(|cells| cells.get(index))?;
    if is_quick_lookable(value, &vm.columns[index].data_type) {
        Some((row, column.to_owned()))
    } else {
        None
    }
}

/// Navigation guard: a navigation re-read is refused while the editor holds an
/// invalid draft (React `TableView.navigationGuard.test.tsx`).
#[allow(dead_code)] // pure gate; consumed by the host and tests.
pub fn navigation_allowed(vm: &TableViewVm) -> bool {
    !vm.invalid_draft
}

/// The insert notice is scoped to one commit of one page. Any page-move event
/// invalidates it; a later successful locate replaces it.
#[allow(dead_code)] // pure reducer; consumed by the host and tests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NoticeEvent {
    CommitNotLocated,
    CommitLocated(usize),
    PageChanged,
    LoadRetried,
    Reloaded,
    ContextChanged,
}

#[allow(dead_code)] // pure reducer; consumed by the host and tests.
pub fn reduce_notice(_current: Option<&str>, event: NoticeEvent) -> (Option<String>, Option<usize>) {
    match event {
        NoticeEvent::CommitNotLocated => (
            Some(t("tableData.insertReturningUnavailable").to_owned()),
            None,
        ),
        NoticeEvent::CommitLocated(row) => (None, Some(row)),
        NoticeEvent::PageChanged
        | NoticeEvent::LoadRetried
        | NoticeEvent::Reloaded
        | NoticeEvent::ContextChanged => (None, None),
    }
}

/// React's 3000ms read-only tip: expired when the recorded deadline passes.
#[allow(dead_code)] // pure helper; consumed by the host poll and tests.
pub fn tip_expired(vm: &TableViewVm, now: f64) -> bool {
    vm.read_only_tip_until.is_some_and(|until| now >= until)
}
