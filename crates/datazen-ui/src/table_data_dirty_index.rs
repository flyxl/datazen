//! `stores/tableData/dirtyIndex.ts` (T127) ported to Rune.
//!
//! The dirty-state index of one table panel — the single derived view of
//! "what has been staged but not committed yet" (DB-06): the per-row dirty
//! state by row identity, the off-page staged keys, and the counts. A pure
//! function module on purpose, exactly as the React source: it adds no
//! state and no source of truth (`pending_changes` stays the only truth),
//! and it never reads the selection state — `data-dt-selected`
//! (selection) and `data-dt-dirty` (staged change) are orthogonal
//! dimensions (00-contracts §4).
//!
//! # T629 placeholder (must be read before extending)
//!
//! `buildRowIdentity` / `rowIdentityKey` / `duplicateRowIdentityKeys` /
//! `valuesEqual` (and the `stableSerialize` / `valueForColumn` they use)
//! live in React's `lib/tableChanges.ts`, whose real port is **T629
//! (segment 2, pending)** — a cross-segment dependency. Following the
//! `transferMappingView` / T561 / T403 precedent, the needed subset is
//! carried here as pure functions transliterated line for line from the
//! TypeScript source. **Do not extend them with new semantics here**: the
//! real T629 port replaces them at the seams, and until then this module
//! is the only thing that may call them.
//!
//! # Deliberate differences from React
//!
//! * **`off_page_keys` is sorted.** React iterates `pendingChanges.keys()`
//!   in insertion order; the port's `HashMap` has no such order, and the
//!   list's semantics ("staged changes not on the current page, never
//!   dropped") never depend on it — sorted output is deterministic for
//!   the tests instead. The [`DirtyRowState::Inserted`] member stays in
//!   the enum exactly as React documents it (DB-04 produces it on a draft
//!   row; this chapter never does).
//! * **`stableSerialize`'s number branch follows `JSON.stringify`.** A
//!   whole float serializes as the integer (`2.0` → `2`), which is what
//!   `JSON.stringify` does and `serde_json`'s own `Display` does not; the
//!   non-finite arm is unreachable (a `serde_json` number is always
//!   finite) and is documented rather than modelled.
//! * **`rowIdentityKey`'s panic arms are `None`.** React throws on an
//!   empty identity or a NULL/unstable value; the port returns `None` so
//!   callers handle the impossible the same way the React caller would
//!   have handled the throw. The empty-identity arm is genuinely
//!   unreachable through [`build_row_identity`] (it refuses a missing
//!   primary key first).
//! * **Structural sharing returns a clone of the previous entry.** Rust
//!   has no object identity to preserve, so [`row_entry`] returns the
//!   previous entry's data unchanged when nothing about it changed — the
//!   byte-for-byte equivalence the React memo relies on.

use std::collections::HashMap;

use serde_json::Value;

use crate::table_data_connection_state::{ColumnSchema, PendingRowChange, TableState, WriteExpressionId};

/// React `DirtyRowState` (`dirtyIndex.ts:32`): `dirty` (uncommitted column
/// changes), `deleted` (marked for deletion — its cells carry no dirty
/// mark, the same fact is never expressed twice), `inserted` (produced by
/// DB-04 on a draft row, **not by this chapter**).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DirtyRowState {
    Dirty,
    Deleted,
    Inserted,
}

/// React `DirtyCell` (`dirtyIndex.ts:39-55`): one read-only snapshot of an
/// uncommitted cell change. The column is a name, not an index: the index
/// loses its meaning when columns are hidden.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DirtyCell {
    pub column_name: String,
    /// The value before the change (`Value::Null` means SQL NULL).
    pub original: Value,
    /// The value the change would write (`Value::Null` means SQL NULL).
    pub next: Value,
    /// The staged write expression of this column (DBR-05), when the write
    /// is a typed database expression rather than a value.
    pub expression: Option<WriteExpressionId>,
}

/// React `DirtyRowEntry` (`dirtyIndex.ts:57-65`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DirtyRowEntry {
    pub identity_key: String,
    pub state: DirtyRowState,
    /// The columns that really changed. Empty for a deleted row.
    pub changed_columns: Vec<String>,
    /// One snapshot per changed column, in the same order.
    pub cells: Vec<DirtyCell>,
}

/// React `DirtyCounts` (`dirtyIndex.ts:67-81`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct DirtyCounts {
    pub update_rows: usize,
    pub delete_rows: usize,
    pub total_rows: usize,
    pub total_cells: usize,
    pub cells_on_page: usize,
    pub cells_off_page: usize,
}

/// React `DirtyIndex` (`dirtyIndex.ts:83-89`).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct DirtyIndex {
    /// Current page row index → that row's dirty state.
    pub by_row_index: HashMap<usize, DirtyRowEntry>,
    /// Staged changes that are not on the current page (they are never
    /// dropped). Sorted for determinism (see Deliberate differences).
    pub off_page_keys: Vec<String>,
    pub counts: DirtyCounts,
}

// ---------------------------------------------------------------------------
// T629 placeholder: the `lib/tableChanges.ts` identity subset.
// ---------------------------------------------------------------------------

/// `valueForColumn` (`tableChanges.ts:188-191`): the row's value, with an
/// absent column reading as null (React's `undefined → null`).
fn value_for_column(row: &HashMap<String, Value>, column: &str) -> Value {
    row.get(column).cloned().unwrap_or(Value::Null)
}

/// `stableSerialize` (`tableChanges.ts:199-238`): a JSON-shaped
/// canonicalizer whose object keys are sorted (byte order approximating
/// JS `Object.keys().sort()`'s UTF-16 code-unit order) and whose numbers
/// follow `JSON.stringify` — a whole float serializes as the integer. A
/// `serde_json` value is always finite, so the non-finite arm of the
/// React source is unreachable here.
fn stable_serialize(value: &Value) -> Option<String> {
    match value {
        Value::Null => Some("null".to_owned()),
        Value::Bool(flag) => Some(flag.to_string()),
        Value::Number(number) => Some(number_to_json_string(number)),
        Value::String(text) => serde_json::to_string(text).ok(),
        Value::Array(items) => {
            let mut out = String::from("[");
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(&stable_serialize(item)?);
            }
            out.push(']');
            Some(out)
        }
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let mut out = String::from("{");
            for (index, key) in keys.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                // `JSON.stringify(key)`: the key's JSON string form.
                let key_json = serde_json::to_string(key.as_str()).ok()?;
                out.push_str(&key_json);
                out.push(':');
                out.push_str(&stable_serialize(map.get(*key)?)?);
            }
            out.push('}');
            Some(out)
        }
    }
}

/// `JSON.stringify(number)`: integers as-is; a whole finite float as the
/// integer it equals (JS prints `2.0` as `2`). Non-integers print the
/// shortest float form `serde_json` produces.
fn number_to_json_string(number: &serde_json::Number) -> String {
    if let Some(as_f64) = number.as_f64() {
        if let Some(as_i64) = float_whole_to_i64(as_f64) {
            return as_i64.to_string();
        }
    }
    number.to_string()
}

/// A finite float whose fraction is zero converts to the integer it
/// equals, when it fits the JS safe range.
fn float_whole_to_i64(value: f64) -> Option<i64> {
    if value.fract() == 0.0 && value.abs() <= 9_007_199_254_740_991.0 {
        Some(value as i64)
    } else {
        None
    }
}

/// `isStableIdentityValue` (`tableChanges.ts:241-243`): a value that is
/// not null and survives [`stable_serialize`]. A `serde_json` value
/// always serializes stably, so this is the null check.
fn is_stable_identity_value(value: &Value) -> bool {
    !value.is_null()
}

/// React `RowIdentity` (`tableChanges.ts:5`): the immutable values that
/// identify a row for an UPDATE or DELETE, keyed by primary-key column
/// name.
#[allow(dead_code)] // T629 replaces this placeholder; the dirty index is its only caller
pub(crate) type RowIdentity = HashMap<String, Value>;

/// `buildRowIdentity` (`tableChanges.ts:247-259`): a stable identity from
/// the table's declared primary-key columns, or `None` when any of them
/// holds null.
#[allow(dead_code)] // T629 replaces this placeholder
pub(crate) fn build_row_identity(
    row: &HashMap<String, Value>,
    primary_key_columns: &[ColumnSchema],
) -> Option<RowIdentity> {
    if primary_key_columns.is_empty() {
        return None;
    }
    let mut identity = RowIdentity::new();
    for column in primary_key_columns {
        let value = value_for_column(row, &column.name);
        if !is_stable_identity_value(&value) {
            return None;
        }
        identity.insert(column.name.clone(), value);
    }
    Some(identity)
}

/// `rowIdentityKey` (`tableChanges.ts:262-276`): the deterministic map
/// key — column order in a schema must not affect identity. `None` is
/// React's throw on an empty identity or a NULL/unstable value (see
/// Deliberate differences).
#[allow(dead_code)] // T629 replaces this placeholder
pub(crate) fn row_identity_key(identity: &RowIdentity) -> Option<String> {
    if identity.is_empty() {
        return None;
    }
    let mut keys: Vec<&String> = identity.keys().collect();
    keys.sort();
    let mut entries = Vec::new();
    for key in keys {
        let value = identity.get(key)?;
        if !is_stable_identity_value(value) {
            return None;
        }
        let serialized = stable_serialize(value)?;
        let key_json = serde_json::to_string(key.as_str()).ok()?;
        entries.push(format!("{key_json}:{serialized}"));
    }
    Some(format!("[{}]", entries.join(",")))
}

/// `duplicateRowIdentityKeys` (`tableChanges.ts:280-295`): the duplicate
/// stable identities in a loaded row set, sorted.
#[allow(dead_code)] // T629 replaces this placeholder
pub(crate) fn duplicate_row_identity_keys(
    rows: &[HashMap<String, Value>],
    primary_key_columns: &[ColumnSchema],
) -> Vec<String> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for row in rows {
        let Some(identity) = build_row_identity(row, primary_key_columns) else {
            continue;
        };
        if let Some(key) = row_identity_key(&identity) {
            *counts.entry(key).or_insert(0) += 1;
        }
    }
    let mut duplicates: Vec<String> = counts
        .into_iter()
        .filter(|(_, count)| *count > 1)
        .map(|(key, _)| key)
        .collect();
    duplicates.sort();
    duplicates
}

/// `valuesEqual` (`tableChanges.ts:297-301`): both nulls read as equal
/// null; otherwise the serialized forms decide.
#[allow(dead_code)] // T629 replaces this placeholder
pub(crate) fn values_equal(left: &Value, right: &Value) -> bool {
    stable_serialize(left) == stable_serialize(right)
}

// ---------------------------------------------------------------------------
// The dirty index itself.
// ---------------------------------------------------------------------------

/// React `dirtyCellKey` (`dirtyIndex.ts:96-118`): the composite key of a
/// cell-level dirty mark — row identity key + NUL + column name. The NUL
/// separator is impossible on both sides (identity keys escape it, SQL
/// identifiers cannot contain it), so the key space stays injective.
///
/// **This is a contract symbol, not a dead export**: 06 §3.1 freezes it as
/// *the* key of a cell-level dirty mark; a later chapter that addresses
/// one changed cell by row identity + column name must build exactly this
/// key, otherwise two chapters would build different keys for the same
/// cell.
#[allow(dead_code)] // 06 §3.1 contract symbol; the cell-level consumer lands later
pub(crate) fn dirty_cell_key(identity_key: &str, column_name: &str) -> String {
    let mut key = String::with_capacity(identity_key.len() + column_name.len() + 1);
    key.push_str(identity_key);
    key.push('\u{0}');
    key.push_str(column_name);
    key
}

/// React `EMPTY_DIRTY_INDEX`: the stable empty index, built once.
#[allow(dead_code)] // reusable index; the grid render track wires it
pub(crate) fn empty_dirty_index() -> &'static DirtyIndex {
    static EMPTY: std::sync::OnceLock<DirtyIndex> = std::sync::OnceLock::new();
    EMPTY.get_or_init(DirtyIndex::default)
}

/// React `reallyChangedColumns` (`dirtyIndex.ts:129-139`): the columns of
/// a staged change that really differ. `stageCellChange` already drops a
/// column when the user reverts it, so this filter is the second line of
/// defence — a staged expression is a change even when its placeholder
/// equals the original (the intent is the expression).
fn really_changed_columns(change: &PendingRowChange) -> Vec<String> {
    change
        .changed_columns
        .iter()
        .filter(|column| {
            let staged_expression = change
                .expression_values
                .as_ref()
                .is_some_and(|map| map.get(*column).is_some());
            staged_expression
                || !values_equal(
                    change.original_values.get(*column).unwrap_or(&Value::Null),
                    change.current_values.get(*column).unwrap_or(&Value::Null),
                )
        })
        .cloned()
        .collect()
}

/// React `selectDirtyCounts` (`dirtyIndex.ts:151-172`): the counts that
/// depend on the staged changes alone — it deliberately knows nothing
/// about the current page, so a panel whose rows were dropped or that has
/// not loaded yet still reports the right number of staged changes.
#[allow(dead_code)] // reusable selector; the panel header wires it
pub(crate) fn select_dirty_counts(
    pending_changes: &HashMap<String, PendingRowChange>,
) -> DirtyCounts {
    let mut update_rows = 0;
    let mut delete_rows = 0;
    let mut total_cells = 0;
    for change in pending_changes.values() {
        if change.delete_marked {
            delete_rows += 1;
            continue;
        }
        let columns = really_changed_columns(change);
        if columns.is_empty() {
            continue;
        }
        update_rows += 1;
        total_cells += columns.len();
    }
    DirtyCounts {
        update_rows,
        delete_rows,
        total_rows: update_rows + delete_rows,
        total_cells,
        cells_on_page: 0,
        cells_off_page: total_cells,
    }
}

/// React `selectDirtyRow` (`dirtyIndex.ts:174-178`): one row's dirty
/// state, or `None` when the row is clean.
#[allow(dead_code)] // reusable selector; the grid render track wires it
pub(crate) fn select_dirty_row(index: Option<&DirtyIndex>, row_index: usize) -> Option<&DirtyRowEntry> {
    index?.by_row_index.get(&row_index)
}

/// React `sameCells` (`dirtyIndex.ts:180-189`): two cell lists agree
/// column by column (name, original, next).
fn same_cells(left: &[DirtyCell], right: &[DirtyCell]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter().zip(right.iter()).all(|(cell, other)| {
        cell.column_name == other.column_name
            && values_equal(&cell.original, &other.original)
            && values_equal(&cell.next, &other.next)
    })
}

/// React `rowEntry` (`dirtyIndex.ts:191-224`): the entry of one row,
/// reusing the previous object when nothing about it changed — the
/// structural sharing that keeps `VirtualRow`'s memo alive. The port
/// returns the previous entry's data unchanged (see Deliberate
/// differences).
fn row_entry(
    identity_key: &str,
    change: &PendingRowChange,
    previous_by_identity: &HashMap<String, DirtyRowEntry>,
) -> DirtyRowEntry {
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
    let cells: Vec<DirtyCell> = changed_columns
        .iter()
        .map(|column_name| DirtyCell {
            column_name: column_name.clone(),
            original: change
                .original_values
                .get(column_name)
                .cloned()
                .unwrap_or(Value::Null),
            next: change
                .current_values
                .get(column_name)
                .cloned()
                .unwrap_or(Value::Null),
            expression: change
                .expression_values
                .as_ref()
                .and_then(|map| map.get(column_name).copied()),
        })
        .collect();

    if let Some(previous) = previous_by_identity.get(identity_key) {
        if previous.state == state
            && previous.changed_columns.len() == changed_columns.len()
            && previous
                .changed_columns
                .iter()
                .zip(changed_columns.iter())
                .all(|(a, b)| a == b)
            && same_cells(&previous.cells, &cells)
        {
            return previous.clone();
        }
    }

    DirtyRowEntry {
        identity_key: identity_key.to_owned(),
        state,
        changed_columns,
        cells,
    }
}

/// React `buildDirtyIndex` (`dirtyIndex.ts:226-313`): the dirty index of
/// a panel from its table state. Only `rows` / `columns` /
/// `pending_changes` / `row_identity_anchors` are read — never the
/// selection.
///
/// The anchor wins over the recomputed identity (the same priority
/// `findPendingForRow` uses), rows with an ambiguous identity are never
/// marked (a change on one of two rows with the same key would be a
/// guess), and every staged change that matched no page row lands on
/// `off_page_keys`.
#[allow(dead_code)] // reusable index; the grid render track wires it
pub(crate) fn build_dirty_index(ts: &TableState, previous: Option<&DirtyIndex>) -> DirtyIndex {
    if ts.pending_changes.is_empty() {
        return empty_dirty_index().clone();
    }

    let base = select_dirty_counts(&ts.pending_changes);
    let pk_columns: Vec<ColumnSchema> = ts
        .columns
        .iter()
        .filter(|column| column.is_primary_key)
        .cloned()
        .collect();

    let mut by_row_index: HashMap<usize, DirtyRowEntry> = HashMap::new();
    let mut off_page_keys: Vec<String> = Vec::new();
    let mut cells_on_page = 0usize;

    if !pk_columns.is_empty() {
        // Rows with an ambiguous identity are never marked: the duplicate
        // keys are computed once for the whole page, not once per row.
        let ambiguous_keys = duplicate_row_identity_keys(&ts.rows, &pk_columns);
        let mut matched_keys: Vec<String> = Vec::new();
        let previous_by_identity = previous_entries(previous);

        for (row_index, row) in ts.rows.iter().enumerate() {
            let Some(identity) = build_row_identity(row, &pk_columns) else {
                continue;
            };
            let Some(identity_key) = row_identity_key(&identity) else {
                continue;
            };
            if ambiguous_keys.contains(&identity_key) {
                continue;
            }

            // The anchor wins over the recomputed identity: it keeps
            // pointing at the original identity of a row whose primary key
            // was edited, so the change stays on the row the user edited.
            let anchored_key = ts.row_identity_anchors.get(&row_index);
            let anchored = anchored_key.and_then(|key| ts.pending_changes.get(key));
            let (key, change) = match (anchored_key, anchored) {
                (Some(key), Some(change)) => (key.clone(), change),
                _ => match ts.pending_changes.get(&identity_key) {
                    Some(change) => (identity_key, change),
                    None => continue,
                },
            };

            matched_keys.push(key.clone());
            let entry = row_entry(&key, change, &previous_by_identity);
            cells_on_page += entry.changed_columns.len();
            by_row_index.insert(row_index, entry);
        }

        for key in ts.pending_changes.keys() {
            if !matched_keys.contains(key) {
                off_page_keys.push(key.clone());
            }
        }
        off_page_keys.sort();
    } else {
        // No primary key: nothing can be staged against this table at all,
        // so every staged change is off-page by definition.
        for key in ts.pending_changes.keys() {
            off_page_keys.push(key.clone());
        }
        off_page_keys.sort();
    }

    DirtyIndex {
        by_row_index,
        off_page_keys,
        counts: DirtyCounts {
            cells_on_page,
            cells_off_page: base.total_cells.saturating_sub(cells_on_page),
            ..base
        },
    }
}

/// React `previousEntries` (`dirtyIndex.ts:316-324`): the previous
/// round's entries, indexed by identity key — built once per round so
/// the lookup stays linear.
fn previous_entries(previous: Option<&DirtyIndex>) -> HashMap<String, DirtyRowEntry> {
    let mut by_identity = HashMap::new();
    if let Some(previous) = previous {
        for entry in previous.by_row_index.values() {
            by_identity.insert(entry.identity_key.clone(), entry.clone());
        }
    }
    by_identity
}
