//! Pure logic of `windows/connection/useDirtyIndex.ts` and its derivation
//! module `stores/tableData/dirtyIndex.ts` (DB-06).
//!
//! The dirty index is *derived*, never stored: a pure function of a panel's
//! staged changes and its current page. The React hook only memoises that pure
//! function and keeps the previous round for structural sharing. Both are
//! modelled here over a host-owned snapshot: [`build_dirty_index`] takes the
//! current table state and an optional previous index and returns the new one.
//! No store, IPC, timer or selection state is read — dirty marks and selection
//! are orthogonal dimensions.
//!
//! One documented deviation: `valuesEqual` in React uses `JSON.stringify`,
//! which preserves object insertion order; `serde_json::Value` orders object
//! keys, so two objects with the same pairs in a different order compare equal
//! here. Scalar null / NaN / type-sensitive comparisons are exact.
//!
//! ## Removal stage
//!
//! No production host mounts the grid yet; every reusable item carries a
//! per-item `#[allow(dead_code)]` with removal stage "drop when the table host
//! owns the dirty index".

use std::collections::{BTreeMap, HashSet};
use std::rc::Rc;

use serde_json::Value;

/// A staged expression id (DBR-05): an expression is a change even when its
/// placeholder value equals the original.
#[allow(dead_code)] // VM mirror; remove when the host owns change requests.
pub(crate) type ExpressionId = String;

/// One uncommitted row change (`PendingRowChange` in React).
#[allow(dead_code)] // VM mirror; supplied by the host's change map.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct PendingChangeVm {
    pub(crate) row_identity: BTreeMap<String, Value>,
    pub(crate) original_values: BTreeMap<String, Value>,
    pub(crate) current_values: BTreeMap<String, Value>,
    pub(crate) changed_columns: Vec<String>,
    pub(crate) delete_marked: bool,
    pub(crate) expression_values: BTreeMap<String, ExpressionId>,
}

/// A declared column, reduced to the field the identity builder reads.
#[allow(dead_code)] // VM mirror; supplied by the host result feed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct DirtyColumnVm {
    pub(crate) name: String,
    pub(crate) is_primary_key: bool,
}

/// The panel slice the derivation reads. Selection is deliberately absent.
#[allow(dead_code)] // VM surface; owned by the table host.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct DirtyTableVm {
    pub(crate) columns: Vec<DirtyColumnVm>,
    pub(crate) rows: Vec<BTreeMap<String, Value>>,
    pub(crate) pending_changes: BTreeMap<String, PendingChangeVm>,
    /// Row index -> the identity key at edit time (primary-key edit anchor).
    pub(crate) row_identity_anchors: BTreeMap<usize, String>,
}

/// React `DirtyRowState`. `inserted` is reserved for DB-04, never produced here.
#[allow(dead_code)] // contract mirror; remove when drafts join the index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DirtyRowState {
    Dirty,
    Deleted,
}

/// One uncommitted cell change, by column name.
#[allow(dead_code)] // VM mirror; consumed by grid markup and tests.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DirtyCellVm {
    pub(crate) column_name: String,
    pub(crate) original: Value,
    pub(crate) next: Value,
    pub(crate) expression: Option<ExpressionId>,
}

/// One row's resolved dirty state (identity-keyed, not column-indexed).
#[allow(dead_code)] // VM mirror; consumed by grid markup and tests.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DirtyRowEntryVm {
    pub(crate) identity_key: String,
    pub(crate) state: DirtyRowState,
    pub(crate) changed_columns: Vec<String>,
    pub(crate) cells: Vec<DirtyCellVm>,
}

/// Counts of staged work, split by current page.
#[allow(dead_code)] // VM mirror; consumed by the pending bar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct DirtyCountsVm {
    pub(crate) update_rows: usize,
    pub(crate) delete_rows: usize,
    pub(crate) total_rows: usize,
    pub(crate) total_cells: usize,
    pub(crate) cells_on_page: usize,
    pub(crate) cells_off_page: usize,
}

/// The derived dirty index. Entries are `Rc` so untouched rows keep object
/// identity across an edit in another row (React's structural sharing).
#[allow(dead_code)] // derived result; remove when the host owns the grid.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct DirtyIndexVm {
    pub(crate) by_row_index: BTreeMap<usize, Rc<DirtyRowEntryVm>>,
    pub(crate) off_page_keys: Vec<String>,
    pub(crate) counts: DirtyCountsVm,
}

/// React `dirtyCellKey`: identity key + NUL + column name. NUL never occurs in
/// a JSON-shaped identity key or a SQL identifier, so the join is injective.
#[allow(dead_code)] // contract symbol; remove when cell-level marks land.
pub(crate) fn dirty_cell_key(identity_key: &str, column_name: &str) -> String {
    format!("{identity_key}\0{column_name}")
}

/// React `valuesEqual`: `JSON.stringify(left ?? null) === JSON.stringify(right ?? null)`.
/// `None` (undefined) and `Null` normalise to the same value.
#[allow(dead_code)] // pure helper; remove when the shared helper lands.
pub(crate) fn values_equal(left: Option<&Value>, right: Option<&Value>) -> bool {
    let normalize = |value: Option<&Value>| value.cloned().unwrap_or(Value::Null);
    let (left, right) = (normalize(left), normalize(right));
    serde_json::to_string(&left).unwrap_or_default() == serde_json::to_string(&right).unwrap_or_default()
}

/// `JSON.stringify` for a string: escape quotes, backslashes and control
/// characters. Mirrors `query_editor_section/context.rs::json_string`.
fn json_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            other if (other as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", other as u32)),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// React `stableSerialize`: scalars are stable, NULL / non-finite numbers are
/// not; objects and arrays are stable when all their leaves are stable.
fn stable_serialize(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::Bool(value) => Some(value.to_string()),
        Value::Number(value) if value.is_f64() && !value.as_f64().is_some_and(f64::is_finite) => None,
        Value::Number(value) => Some(value.to_string()),
        Value::String(value) => Some(json_string(value)),
        Value::Array(values) => {
            let parts: Option<Vec<String>> = values.iter().map(stable_serialize).collect();
            parts.map(|parts| format!("[{}]", parts.join(",")))
        }
        Value::Object(map) => {
            let mut parts = Vec::new();
            for (key, value) in map {
                parts.push(format!("\"{key}\":{}", stable_serialize(value)?));
            }
            Some(format!("{{{}}}", parts.join(",")))
        }
    }
}

/// React `buildRowIdentity`: all primary-key columns must carry a stable,
/// non-NULL value; otherwise the row has no usable identity.
fn build_row_identity(
    row: &BTreeMap<String, Value>,
    pk_columns: &[DirtyColumnVm],
) -> Option<BTreeMap<String, Value>> {
    if pk_columns.is_empty() {
        return None;
    }
    let mut identity = BTreeMap::new();
    for column in pk_columns {
        let value = row.get(&column.name)?;
        if stable_serialize(value).is_none() {
            return None;
        }
        identity.insert(column.name.clone(), value.clone());
    }
    Some(identity)
}

/// React `rowIdentityKey`: sorted columns, `"column":serialized` pairs.
fn row_identity_key(identity: &BTreeMap<String, Value>) -> String {
    let entries: Vec<String> = identity
        .iter()
        .map(|(column, value)| {
            let serialized = stable_serialize(value).unwrap_or_else(|| "null".to_owned());
            format!("\"{column}\":{serialized}")
        })
        .collect();
    format!("[{}]", entries.join(","))
}

/// React `duplicateRowIdentityKeys`: identities appearing more than once are
/// ambiguous and must never be marked dirty.
fn duplicate_row_identity_keys(
    rows: &[BTreeMap<String, Value>],
    pk_columns: &[DirtyColumnVm],
) -> HashSet<String> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for row in rows {
        if let Some(identity) = build_row_identity(row, pk_columns) {
            *counts.entry(row_identity_key(&identity)).or_insert(0) += 1;
        }
    }
    counts.into_iter().filter(|(_, count)| *count > 1).map(|(key, _)| key).collect()
}

/// React `reallyChangedColumns`: a staged expression is a change even when its
/// placeholder equals the original; otherwise compare via `valuesEqual`.
fn really_changed_columns(change: &PendingChangeVm) -> Vec<String> {
    change
        .changed_columns
        .iter()
        .filter(|column| {
            change.expression_values.contains_key(column.as_str())
                || !values_equal(
                    change.original_values.get(column.as_str()),
                    change.current_values.get(column.as_str()),
                )
        })
        .cloned()
        .collect()
}

/// React `selectDirtyCounts`: staged-work counts independent of the page.
#[allow(dead_code)] // pure helper; remove when the host owns counts.
pub(crate) fn select_dirty_counts(pending: &BTreeMap<String, PendingChangeVm>) -> DirtyCountsVm {
    let mut counts = DirtyCountsVm::default();
    for change in pending.values() {
        if change.delete_marked {
            counts.delete_rows += 1;
            continue;
        }
        let columns = really_changed_columns(change);
        if columns.is_empty() {
            continue;
        }
        counts.update_rows += 1;
        counts.total_cells += columns.len();
    }
    counts.total_rows = counts.update_rows + counts.delete_rows;
    counts.cells_off_page = counts.total_cells;
    counts
}

/// React `selectDirtyRow`: the entry for a current-page row index.
#[allow(dead_code)] // pure helper; remove when the grid reads marks.
pub(crate) fn select_dirty_row(index: Option<&DirtyIndexVm>, row: usize) -> Option<Rc<DirtyRowEntryVm>> {
    index?.by_row_index.get(&row).cloned()
}

fn same_cells(left: &[DirtyCellVm], right: &[DirtyCellVm]) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(a, b)| {
            a.column_name == b.column_name
                && values_equal(Some(&a.original), Some(&b.original))
                && values_equal(Some(&a.next), Some(&b.next))
        })
}

/// React `rowEntry`, reusing the previous object when nothing changed.
fn row_entry(
    identity_key: String,
    change: &PendingChangeVm,
    previous_by_identity: &BTreeMap<String, Rc<DirtyRowEntryVm>>,
) -> Rc<DirtyRowEntryVm> {
    let state = if change.delete_marked {
        DirtyRowState::Deleted
    } else {
        DirtyRowState::Dirty
    };
    let changed_columns = if change.delete_marked {
        Vec::new()
    } else {
        really_changed_columns(change)
    };
    let cells: Vec<DirtyCellVm> = changed_columns
        .iter()
        .map(|column_name| {
            let expression = change.expression_values.get(column_name).cloned();
            DirtyCellVm {
                column_name: column_name.clone(),
                original: change.original_values.get(column_name).cloned().unwrap_or(Value::Null),
                next: change.current_values.get(column_name).cloned().unwrap_or(Value::Null),
                expression,
            }
        })
        .collect();

    if let Some(previous) = previous_by_identity.get(&identity_key) {
        if previous.state == state
            && previous.changed_columns == changed_columns
            && same_cells(&previous.cells, &cells)
        {
            return previous.clone();
        }
    }
    Rc::new(DirtyRowEntryVm {
        identity_key,
        state,
        changed_columns,
        cells,
    })
}

/// React `buildDirtyIndex`: the whole-page batch derivation, with the anchor
/// taking priority over a recomputed identity, ambiguous rows skipped, and
/// structural sharing for untouched entries.
#[allow(dead_code)] // view entry; remove when the host owns the dirty index.
pub(crate) fn build_dirty_index(ts: &DirtyTableVm, previous: Option<&DirtyIndexVm>) -> DirtyIndexVm {
    if ts.pending_changes.is_empty() {
        return DirtyIndexVm::default();
    }
    let mut counts = select_dirty_counts(&ts.pending_changes);
    let pk_columns: Vec<DirtyColumnVm> = ts
        .columns
        .iter()
        .filter(|column| column.is_primary_key)
        .cloned()
        .collect();

    let mut by_row_index = BTreeMap::new();
    let mut off_page_keys = Vec::new();

    if !pk_columns.is_empty() {
        let ambiguous = duplicate_row_identity_keys(&ts.rows, &pk_columns);
        let mut matched_keys = HashSet::new();
        let previous_by_identity: BTreeMap<String, Rc<DirtyRowEntryVm>> = previous
            .map(|index| {
                index
                    .by_row_index
                    .values()
                    .map(|entry| (entry.identity_key.clone(), entry.clone()))
                    .collect()
            })
            .unwrap_or_default();

        for (row_index, row) in ts.rows.iter().enumerate() {
            let Some(identity) = build_row_identity(row, &pk_columns) else {
                continue;
            };
            let identity_key = row_identity_key(&identity);
            if ambiguous.contains(&identity_key) {
                continue;
            }
            let anchored_key = ts.row_identity_anchors.get(&row_index);
            let anchored = anchored_key.and_then(|key| ts.pending_changes.get(key));
            let key = match (anchored_key, anchored) {
                (Some(anchored_key), Some(_)) => anchored_key.clone(),
                _ => identity_key.clone(),
            };
            let change = match anchored {
                Some(change) => Some(change),
                None => ts.pending_changes.get(&identity_key),
            };
            let Some(change) = change else {
                continue;
            };
            matched_keys.insert(key.clone());
            let entry = row_entry(key, change, &previous_by_identity);
            counts.cells_on_page += entry.changed_columns.len();
            by_row_index.insert(row_index, entry);
        }

        for key in ts.pending_changes.keys() {
            if !matched_keys.contains(key) {
                off_page_keys.push(key.clone());
            }
        }
    } else {
        // Without a primary key nothing can be staged against this table, so
        // every staged change is off-page by definition.
        off_page_keys.extend(ts.pending_changes.keys().cloned());
    }

    counts.cells_off_page = counts.total_cells.saturating_sub(counts.cells_on_page);
    DirtyIndexVm {
        by_row_index,
        off_page_keys,
        counts,
    }
}
