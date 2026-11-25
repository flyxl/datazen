//! The pending-changes helpers over the table state, ported from React
//! `packages/frontend/src/stores/tableData/pendingChanges.ts` (T275).
//!
//! Eight functions, all pure over [`TableState`] (the T125 mirror) and
//! the T629 placeholder's identity helpers
//! ([`crate::table_data_dirty_index`]): the effective identity of a
//! staged change (its current values over its row identity), the
//! uniqueness and collision checks that gate staging an UPDATE/DELETE,
//! the anchor-first row lookup, the overlay that merges staged edits
//! into the loaded rows, the two wire shapes (staged and drafts), and
//! the two signatures (staged, and the whole pending work).
//!
//! ## What maps where
//!
//! | React | here |
//! |---|---|
//! | `AMBIGUOUS_ROW_IDENTITY_ERROR` | [`AMBIGUOUS_ROW_IDENTITY_ERROR`] (the error's own text is the staging caller's, T278's) |
//! | `effectivePendingIdentity(change, pkColumns)` | [`effective_pending_identity`] |
//! | `rowIdentityIsUnique(ts, rowIndex, identity, pkColumns)` | [`row_identity_is_unique`] |
//! | `hasPendingIdentityCollision(ts, ownKey, identity, pkColumns)` | [`has_pending_identity_collision`] |
//! | `findPendingForRow(...)` | [`find_pending_for_row`] |
//! | `overlayPendingRows(ts, rows)` | [`overlay_pending_rows`] |
//! | `pendingChangesSignature(map)` | [`pending_changes_signature`] |
//! | `pendingChangesForWire(map)` | [`pending_changes_for_wire`] |
//! | `hasPendingWork(ts)` | [`has_pending_work`] |
//! | `draftsForWire(ts)` | [`drafts_for_wire`] |
//! | `pendingWorkSignature({…})` | [`pending_work_signature`] |
//!
//! ## Documented deviations from React
//!
//! * **`JSON.stringify` becomes serde_json.** The two signatures
//!   serialize with `serde_json::to_string` — the same textual shape
//!   the JS engines emit for these value kinds (objects with
//!   string keys, JSON scalars; the key sort the T629 placeholder's
//!   `stable_serialize` pins keeps the maps deterministic).
//! * **`localeCompare` becomes byte ordering.** The staged signature
//!   sorts the entries by key; `String`'s own ordering is the byte
//!   order, which for these ASCII keys is the collation's (the
//!   zh-CN locale does not re-order them).
//! * **`ts.pendingChanges.entries()` iteration order is the sorted
//!   order.** A JS `Map` iterates insertion order and the function
//!   sorts before serializing, so the input order never survives to
//!   the output; the Rust `HashMap`'s arbitrary order therefore cannot
//!   change the signature either — the sort is the whole contract.
//! * **The wire change drops `rowIndex` by field, not spread**: the
//!   port's `PendingRowChange` mirrors the field list directly and
//!   `pending_changes_for_wire` rebuilds it without `row_index`
//!   (the `…rest` spread's exact meaning on this shape).
//! * **`draftWriteColumns`** rides here (the T629 placeholder's
//!   dirty-index subset does not carry it): the membership list of
//!   the draft's values, sorted — the source's own determinism rule.
//!   T629 replaces this module-local copy with the real port.

use std::collections::HashMap;

use serde_json::{json, Value};

use crate::table_data_connection_state::{
    ColumnSchema, PendingInsert, PendingRowChange, TableState,
};
use crate::table_data_dirty_index::{build_row_identity, row_identity_key, RowIdentity};

/// `AMBIGUOUS_ROW_IDENTITY_ERROR`: the refusal text the staging
/// caller shows (React 12-13, verbatim; the message the port's
/// consumers assert byte for byte).
pub const AMBIGUOUS_ROW_IDENTITY_ERROR: &str =
    "Row identity is NULL, unstable, or ambiguous; UPDATE/DELETE was not staged.";

/// `draftWriteColumns` (`lib/tableChanges.ts:79-81`, the T629
/// placeholder's subset this module owns until T629 lands): the
/// draft's write column list — the membership list of the draft's
/// values, sorted, so the same draft always produces the same plan
/// and the same bound parameters. A column the user cleared is
/// absent from `values` and therefore absent here as well.
fn draft_write_columns(values: &HashMap<String, Value>) -> Vec<String> {
    let mut columns: Vec<String> = values.keys().cloned().collect();
    columns.sort();
    columns
}

/// `effectivePendingIdentity` (React 15-28): the change's effective
/// identity — its `rowIdentity` overlaid with every current value it
/// holds for the target columns (the declared primary keys, or the
/// whole column list when the table declares none — the fallback the
/// React source itself spells).
///
/// `hasOwnProperty` on `currentValues` becomes the `HashMap`'s
/// membership: a present `Value::Null` still overwrites (the property
/// exists); an absent column keeps the row identity's own value.
pub fn effective_pending_identity(
    change: &PendingRowChange,
    pk_columns: &[ColumnSchema],
) -> Option<RowIdentity> {
    let actual_pk_columns: Vec<&ColumnSchema> =
        pk_columns.iter().filter(|c| c.is_primary_key).collect();
    let target_columns: Vec<&ColumnSchema> = if !actual_pk_columns.is_empty() {
        actual_pk_columns
    } else {
        pk_columns.iter().collect()
    };
    let mut values: RowIdentity = change.row_identity.clone();
    for column in &target_columns {
        if let Some(value) = change.current_values.get(&column.name) {
            values.insert(column.name.clone(), value.clone());
        }
    }
    let owned: Vec<ColumnSchema> = target_columns.into_iter().cloned().collect();
    build_row_identity(&values, &owned)
}

/// `rowIdentityIsUnique` (React 30-45): whether the identity belongs to
/// exactly one row in the loaded set (its own index excluded — the
/// caller's row never disqualifies itself).
pub fn row_identity_is_unique(
    ts: &TableState,
    row_index: usize,
    identity: &RowIdentity,
    pk_columns: &[ColumnSchema],
) -> bool {
    let Some(key) = row_identity_key(identity) else {
        return false;
    };
    ts.rows
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != row_index)
        .filter_map(|(_, row)| build_row_identity(row, pk_columns))
        .filter_map(|other| row_identity_key(&other))
        .all(|other| other != key)
}

/// `hasPendingIdentityCollision` (React 47-60): whether any OTHER
/// staged change already owns the identity — including one whose own
/// effective identity is broken (`true` there: an ambiguous sibling
/// blocks staging the same way React's `: true` fallthrough does).
pub fn has_pending_identity_collision(
    ts: &TableState,
    own_key: Option<&str>,
    identity: &RowIdentity,
    pk_columns: &[ColumnSchema],
) -> bool {
    let Some(key) = row_identity_key(identity) else {
        return true;
    };
    ts.pending_changes.iter().any(|(pending_key, change)| {
        if Some(pending_key.as_str()) == own_key {
            return false;
        }
        match effective_pending_identity(change, pk_columns) {
            Some(current) => row_identity_key(&current).as_deref() == Some(key.as_str()),
            None => true,
        }
    })
}

/// `findPendingForRow` (React 62-80): the staged change a row's edit
/// should target — the row-identity anchor first (the PK-edit
/// association), then the identity's own key. `None` when the row has
/// no stable identity, is not unique in the loaded set, or no staged
/// change matches.
pub fn find_pending_for_row(
    ts: &TableState,
    row_index: usize,
    row: &HashMap<String, Value>,
    pk_columns: &[ColumnSchema],
) -> Option<(String, PendingRowChange)> {
    let identity = build_row_identity(row, pk_columns)?;
    if !row_identity_is_unique(ts, row_index, &identity, pk_columns) {
        return None;
    }

    // The anchor first: a PK edit's staged change stays associated with
    // its original row even as the identity drifts.
    if let Some(anchored_key) = ts.row_identity_anchors.get(&row_index) {
        if let Some(anchored) = ts.pending_changes.get(anchored_key) {
            return Some((anchored_key.clone(), anchored.clone()));
        }
    }

    let direct_key = row_identity_key(&identity)?;
    if let Some(direct) = ts.pending_changes.get(&direct_key) {
        return Some((direct_key, direct.clone()));
    }
    None
}

/// `overlayPendingRows` (React 82-92): the loaded rows with every
/// uniquely-identified staged change's current values merged in —
/// unchanged when there are no primary keys or nothing is staged.
pub fn overlay_pending_rows(
    ts: &TableState,
    rows: &[HashMap<String, Value>],
) -> Vec<HashMap<String, Value>> {
    let pk_columns: Vec<ColumnSchema> = ts
        .columns
        .iter()
        .filter(|column| column.is_primary_key)
        .cloned()
        .collect();
    if pk_columns.is_empty() || ts.pending_changes.is_empty() {
        return rows.to_vec();
    }
    rows.iter()
        .enumerate()
        .map(|(row_index, row)| match find_pending_for_row(ts, row_index, row, &pk_columns) {
            Some((_, change)) => {
                let mut merged = row.clone();
                for (column, value) in &change.current_values {
                    merged.insert(column.clone(), value.clone());
                }
                merged
            }
            None => row.clone(),
        })
        .collect()
}

/// `pendingChangesSignature` (React 94-100): the staged changes as one
/// deterministic string — the entries sorted by key, each key paired
/// with its cloned change. The React source relies on JS
/// `JSON.stringify` over its map's sorted entries; the port builds the
/// same JSON shape with sorted keys everywhere (see
/// [`change_to_json`]'s note), so two equal states always produce
/// equal strings — the signature's whole contract.
pub fn pending_changes_signature(
    pending_changes: &HashMap<String, PendingRowChange>,
) -> String {
    let mut entries: Vec<(&String, &PendingRowChange)> = pending_changes.iter().collect();
    entries.sort_by(|left, right| left.0.cmp(right.0));
    let array: Vec<Value> = entries
        .into_iter()
        .map(|(key, change)| json!([key, change_to_json(change)]))
        .collect();
    serde_json::to_string(&Value::Array(array)).unwrap_or_default()
}

/// One change as the signature's JSON member — the wire field set
/// minus the UI-only `rowIndex` (the clone's own shape: the staged
/// signature serializes the change the way `clonePendingRowChange`
/// hands it to `JSON.stringify`, `rowIndex` included there but a
/// stable value the clone always preserves). Keys are sorted so the
/// same map always serializes identically (JS objects serialize in
/// insertion order; both sides are deterministic by construction).
fn change_to_json(change: &PendingRowChange) -> Value {
    json!([
        value_map_to_json(&change.row_identity),
        value_map_to_json(&change.original_values),
        value_map_to_json(&change.current_values),
        change.changed_columns,
        change.delete_marked,
        change.insert_marked,
        change.row_index,
    ])
}

/// A `Record<string, Value>` as JSON with its keys sorted (the JS
/// insertion order is not reproducible; the sorted order is the
/// deterministic equivalent both sides of a comparison use).
fn value_map_to_json(map: &HashMap<String, Value>) -> Value {
    let mut keys: Vec<&String> = map.keys().collect();
    keys.sort();
    let mut object = serde_json::Map::new();
    for key in keys {
        object.insert(key.clone(), map.get(key).cloned().unwrap_or(Value::Null));
    }
    Value::Object(object)
}

/// `pendingChangesForWire` (React 102-107): the staged changes in the
/// wire's order — the map's values, each without its UI-only
/// `rowIndex` (the `…rest` spread's exact meaning on this shape).
///
/// React iterates the `Map` in insertion order; the Rust `HashMap`'s
/// arbitrary order is a deliberate deviation the owning callers pin
/// (the wire contract is the set, and the backend's own request order
/// is the callers' business, T278/T294's).
pub fn pending_changes_for_wire(
    pending_changes: &HashMap<String, PendingRowChange>,
) -> Vec<PendingRowChange> {
    pending_changes
        .values()
        .map(|change| PendingRowChange {
            row_index: None,
            row_identity: change.row_identity.clone(),
            original_values: change.original_values.clone(),
            current_values: change.current_values.clone(),
            changed_columns: change.changed_columns.clone(),
            delete_marked: change.delete_marked,
            insert_marked: change.insert_marked,
            expression_values: change.expression_values.clone(),
        })
        .collect()
}

/// `hasPendingWork` (React 119-121, the doc comment verbatim in
/// intent): staged edits **or** new-row drafts — the two collections
/// are independent; a panel can hold drafts without a single staged
/// edit (the normal state right after "+ row"), and a panel can hold
/// edits without drafts. Every gate that used to ask
/// `pendingChanges.size === 0` has to ask this instead.
pub fn has_pending_work(ts: &TableState) -> bool {
    !ts.pending_changes.is_empty() || !ts.draft_inserts.is_empty()
}

/// `draftsForWire` (React 136-153): the drafts as the wire sees them —
/// one `PendingRowChange` per draft, marked `insertMarked: true`, in
/// `draftInsertOrder` (never the map's order: the insertion order
/// decides which row gets which auto-increment value, and the back end
/// keeps the request order). The draft id never goes on the wire; the
/// shape is exactly an existing row change's plus the marker.
pub fn drafts_for_wire(ts: &TableState) -> Vec<PendingRowChange> {
    ts.draft_insert_order
        .iter()
        .filter_map(|draft_id| ts.draft_inserts.get(draft_id))
        .map(|draft: &PendingInsert| PendingRowChange {
            row_index: None,
            row_identity: HashMap::new(),
            original_values: HashMap::new(),
            current_values: draft.values.clone(),
            changed_columns: draft_write_columns(&draft.values),
            delete_marked: false,
            insert_marked: true,
            expression_values: None,
        })
        .collect()
}

/// `pendingWorkSignature` (React 164-174): the signature of everything
/// that has to be committed — the staged part through
/// [`pending_changes_signature`] (kept untouched, its single-argument
/// contract) and the draft part in creation order (for drafts the
/// order *is* part of the intent).
pub fn pending_work_signature(
    pending_changes: &HashMap<String, PendingRowChange>,
    draft_inserts: &HashMap<String, PendingInsert>,
    draft_insert_order: &[String],
) -> String {
    let drafts: Vec<Value> = draft_insert_order
        .iter()
        .map(|draft_id| {
            let draft = draft_inserts.get(draft_id);
            match draft {
                Some(draft) => json!([draft_id, value_map_to_json(&draft.values)]),
                None => json!([draft_id, null]),
            }
        })
        .collect();
    let staged = pending_changes_signature(pending_changes);
    let staged_json: Value = serde_json::from_str(&staged).unwrap_or(Value::Null);
    serde_json::to_string(&json!([staged_json, drafts])).unwrap_or_default()
}
