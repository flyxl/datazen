//! The mapping view's pure functions, ported from React
//! `packages/frontend/src/windows/data-sync/mappingView.ts` (tasklist T417).
//!
//! The React module is the Data Sync wizard's whole read-model over the
//! inspection/comparison results: row-diff counting (wire counters first,
//! rows fallback), the status→label mapping, display/key spellings, the two
//! summaries, the selection-vs-options rules, the table filter, and the
//! two merges (compare→mappings, disabled-marking). Everything is pure; no
//! React hook survives the port.
//!
//! Type mirrors: `DataSyncTableResult` / `DataSyncRowChange` /
//! `DataSyncOperation` / `SyncOptions` / `DataSyncMappingStatus` /
//! `TableDiffFilter` all live in the compare step's port (T024,
//! [`crate::views::data_sync::data_sync_compare_step`]) — they were mirrored
//! from this file's own imports (`commands/sync.ts`) when the compare step
//! landed, so this port re-uses them instead of forking second copies.
//! `Value` is `serde_json::Value`; `TranslationKey` is the `t()` key.
//!
//! ## Trade notes
//!
//! * `row.rows ?? []` — the mirror's `rows: Vec<_>` treats an absent list
//!   and an empty list the same; every count the fallback computes over an
//!   absent list is 0 either way.
//! * `warnings: cmp.warnings ?? m.warnings` — the mirror's `Vec<String>`
//!   cannot distinguish `undefined` from `[]` on the wire; the merge keeps
//!   `cmp`'s warnings when they are non-empty and otherwise falls back to
//!   the mapping's, which matches the React outcome for every payload a
//!   backend actually sends (an absent or empty list means "no warnings").
//! * `markDisabledTables`' `rows: undefined` becomes an emptied `Vec`.
//! * `rowKeyString`'s `try/catch` guards a circular structure — impossible
//!   for `serde_json::Value`, so the fallback is documented away, not
//!   ported.

use serde_json::Value;

use crate::i18n::t;
use crate::views::data_sync::data_sync_compare_step::{
    DataSyncMappingStatus, DataSyncOperation, DataSyncRowChange, DataSyncTableResult, SyncOptions,
    TableDiffFilter,
};

/// The four row-diff counters of one table result.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct RowDiffCounts {
    pub(crate) inserts: usize,
    pub(crate) updates: usize,
    pub(crate) deletes: usize,
    pub(crate) unchanged: usize,
}

/// `rowDiffCounts`: the wire counters win; absent ones are recomputed from
/// the row list (`insertCount ?? count(INSERT)`), while the unchanged
/// counter is `unchangedCount ?? 0` **plus** the UNCHANGED rows in the list.
pub(crate) fn row_diff_counts(row: &DataSyncTableResult) -> RowDiffCounts {
    RowDiffCounts {
        inserts: row.insert_count.unwrap_or_else(|| {
            row.rows
                .iter()
                .filter(|r| r.operation == DataSyncOperation::Insert)
                .count()
        }),
        updates: row.update_count.unwrap_or_else(|| {
            row.rows
                .iter()
                .filter(|r| r.operation == DataSyncOperation::Update)
                .count()
        }),
        deletes: row.delete_count.unwrap_or_else(|| {
            row.rows
                .iter()
                .filter(|r| r.operation == DataSyncOperation::Delete)
                .count()
        }),
        unchanged: row.unchanged_count.unwrap_or(0)
            + row
                .rows
                .iter()
                .filter(|r| r.operation == DataSyncOperation::Unchanged)
                .count(),
    }
}

/// `tableHasRowDiffs`: any of the three change kinds present.
pub(crate) fn table_has_row_diffs(row: &DataSyncTableResult) -> bool {
    let counts = row_diff_counts(row);
    counts.inserts + counts.updates + counts.deletes > 0
}

/// `tableIsUnchanged`: a matched table without row diffs.
pub(crate) fn table_is_unchanged(row: &DataSyncTableResult) -> bool {
    row.status == DataSyncMappingStatus::Matched && !table_has_row_diffs(row)
}

/// `mappingLabelKey`: the zh-CN key of one mapping status's label.
pub(crate) fn mapping_label_key(status: DataSyncMappingStatus) -> &'static str {
    match status {
        DataSyncMappingStatus::Matched => "sync.mappingMatched",
        DataSyncMappingStatus::UnmappedSource => "sync.mappingUnmappedSource",
        DataSyncMappingStatus::UnmappedTarget => "sync.mappingUnmappedTarget",
        DataSyncMappingStatus::Disabled => "sync.mappingDisabled",
        DataSyncMappingStatus::Incompatible => "sync.mappingIncompatible",
    }
}

/// `displayTableName`: `source → target` when both exist and differ;
/// whichever exists otherwise.
pub(crate) fn display_table_name(row: &DataSyncTableResult) -> String {
    if !row.source_table.is_empty()
        && !row.target_table.is_empty()
        && row.source_table != row.target_table
    {
        return format!("{} \u{2192} {}", row.source_table, row.target_table);
    }
    if !row.source_table.is_empty() {
        return row.source_table.clone();
    }
    row.target_table.clone()
}

/// `tableKey`: the source table, or the target when there is none.
pub(crate) fn table_key(row: &DataSyncTableResult) -> String {
    if row.source_table.is_empty() {
        row.target_table.clone()
    } else {
        row.source_table.clone()
    }
}

/// `tablePageKey`: the composite key of a table pair (`source\0target`),
/// used to address the paged comparison cursors of one table.
pub(crate) fn table_page_key(row: &DataSyncTableResult) -> String {
    format!("{}\u{0}{}", row.source_table, row.target_table)
}

/// `summarizeMappings`: the four status buckets of the mapping step.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct MappingSummary {
    pub(crate) matched: usize,
    pub(crate) incompatible: usize,
    pub(crate) unmapped: usize,
    pub(crate) disabled: usize,
}

pub(crate) fn summarize_mappings(rows: &[DataSyncTableResult]) -> MappingSummary {
    let mut summary = MappingSummary::default();
    for row in rows {
        match row.status {
            DataSyncMappingStatus::Matched => summary.matched += 1,
            DataSyncMappingStatus::Incompatible => summary.incompatible += 1,
            DataSyncMappingStatus::UnmappedSource | DataSyncMappingStatus::UnmappedTarget => {
                summary.unmapped += 1
            }
            DataSyncMappingStatus::Disabled => summary.disabled += 1,
        }
    }
    summary
}

/// `summarizeCompare`: the change counters of the comparison step.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct CompareSummaryStats {
    pub(crate) inserts: usize,
    pub(crate) updates: usize,
    pub(crate) deletes: usize,
    pub(crate) unchanged_tables: usize,
    pub(crate) incompatible: usize,
}

pub(crate) fn summarize_compare(rows: &[DataSyncTableResult]) -> CompareSummaryStats {
    let mut stats = CompareSummaryStats::default();
    for row in rows {
        if row.status == DataSyncMappingStatus::Incompatible {
            stats.incompatible += 1;
            continue;
        }
        if row.status != DataSyncMappingStatus::Matched {
            continue;
        }
        let counts = row_diff_counts(row);
        stats.inserts += counts.inserts;
        stats.updates += counts.updates;
        stats.deletes += counts.deletes;
        if !table_has_row_diffs(row) {
            stats.unchanged_tables += 1;
        }
    }
    stats
}

/// `rowKeyString`: the JSON spelling of a composite key. The React
/// `try/catch` guards a circular structure, which `serde_json::Value`
/// cannot represent — the fallback is unreachable and not ported.
pub(crate) fn row_key_string(key: &[Value]) -> String {
    Value::Array(key.to_vec()).to_string()
}

/// `defaultRowSelected`: which operations start selected under options —
/// DELETE never does.
pub(crate) fn default_row_selected(operation: DataSyncOperation, options: &SyncOptions) -> bool {
    match operation {
        DataSyncOperation::Insert => options.insert,
        DataSyncOperation::Update => options.update,
        DataSyncOperation::Delete | DataSyncOperation::Unchanged => false,
    }
}

/// `applyOptionsToRows`: a selected row stays selected only when its
/// operation is still allowed under the options.
pub(crate) fn apply_options_to_rows(
    rows: &[DataSyncRowChange],
    options: &SyncOptions,
) -> Vec<DataSyncRowChange> {
    rows.iter()
        .map(|row| {
            let mut next = row.clone();
            next.selected = row.selected && operation_allowed(row.operation, options);
            next
        })
        .collect()
}

/// `operationAllowed`: the options' three switches.
pub(crate) fn operation_allowed(operation: DataSyncOperation, options: &SyncOptions) -> bool {
    match operation {
        DataSyncOperation::Insert => options.insert,
        DataSyncOperation::Update => options.update,
        DataSyncOperation::Delete => options.delete,
        DataSyncOperation::Unchanged => false,
    }
}

/// `selectedRowCount`: rows that are selected, not UNCHANGED, and still
/// allowed.
pub(crate) fn selected_row_count(table: &DataSyncTableResult, options: &SyncOptions) -> usize {
    table
        .rows
        .iter()
        .filter(|r| {
            r.selected
                && r.operation != DataSyncOperation::Unchanged
                && operation_allowed(r.operation, options)
        })
        .count()
}

/// `tableMatchesFilter`: the query matches the display name
/// case-insensitively, then the status/diff-count filter applies.
pub(crate) fn table_matches_filter(
    row: &DataSyncTableResult,
    filter: TableDiffFilter,
    query: &str,
) -> bool {
    let name = display_table_name(row).to_lowercase();
    if !query.is_empty() && !name.contains(&query.to_lowercase()) {
        return false;
    }
    if filter == TableDiffFilter::All {
        return true;
    }
    if filter == TableDiffFilter::Incompatible {
        return row.status == DataSyncMappingStatus::Incompatible;
    }
    if row.status != DataSyncMappingStatus::Matched {
        return false;
    }
    match filter {
        TableDiffFilter::Insert => row_diff_counts(row).inserts > 0,
        TableDiffFilter::Update => row_diff_counts(row).updates > 0,
        TableDiffFilter::Delete => row_diff_counts(row).deletes > 0,
        TableDiffFilter::Unchanged => table_is_unchanged(row),
        // The two early returns above already handled All and Incompatible.
        TableDiffFilter::All | TableDiffFilter::Incompatible => true,
    }
}

/// `mergeCompareIntoMappings`: fold the comparison results into the
/// inspection mappings by table key. A mapping with no comparison keeps
/// itself; a DISABLED mapping is never overwritten; otherwise every
/// compared field replaces the mapping's, with warnings falling back when
/// the comparison carries none.
pub(crate) fn merge_compare_into_mappings(
    mappings: &[DataSyncTableResult],
    compared: &[DataSyncTableResult],
) -> Vec<DataSyncTableResult> {
    let by_source: std::collections::HashMap<&str, &DataSyncTableResult> = compared
        .iter()
        .map(|r| {
            (
                if r.source_table.is_empty() {
                    r.target_table.as_str()
                } else {
                    r.source_table.as_str()
                },
                r,
            )
        })
        .collect();
    mappings
        .iter()
        .map(|m| {
            let key = if m.source_table.is_empty() {
                m.target_table.as_str()
            } else {
                m.source_table.as_str()
            };
            let Some(cmp) = by_source.get(key) else {
                return m.clone();
            };
            if m.status == DataSyncMappingStatus::Disabled {
                return m.clone();
            }
            let mut merged = (*cmp).clone();
            // `warnings: cmp.warnings ?? m.warnings` — see the module docs'
            // trade note: an empty (absent) list falls back to the mapping's.
            if cmp.warnings.is_empty() {
                merged.warnings = m.warnings.clone();
            }
            merged
        })
        .collect()
}

/// `markDisabledTables`: matched tables in the disabled set become
/// DISABLED with their row list dropped.
pub(crate) fn mark_disabled_tables(
    rows: &[DataSyncTableResult],
    disabled: &std::collections::HashSet<String>,
) -> Vec<DataSyncTableResult> {
    rows.iter()
        .map(|r| {
            let mut next = r.clone();
            if r.status == DataSyncMappingStatus::Matched
                && disabled.contains(&r.source_table)
            {
                next.status = DataSyncMappingStatus::Disabled;
                next.rows = Vec::new();
            }
            next
        })
        .collect()
}

/// `tablesForCompare`: the source tables of matched tables with a
/// non-empty source name (`.filter(Boolean)`).
pub(crate) fn tables_for_compare(rows: &[DataSyncTableResult]) -> Vec<String> {
    rows.iter()
        .filter(|r| r.status == DataSyncMappingStatus::Matched)
        .map(|r| r.source_table.clone())
        .filter(|name| !name.is_empty())
        .collect()
}
