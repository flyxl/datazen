//! New-row draft transitions from `stores/tableData/draftInserts.ts` (T158).
//!
//! Everything here is a **transition on the draft fields of a
//! [`TableState`]**, not an action: the store (the future table-data store
//! track) owns the write, this module owns the rules. Keeping them apart is
//! what keeps the store thin and the rules testable without a store.
//!
//! Two invariants are enforced here and nowhere else:
//!
//! 1. A draft never touches `ts.pending_changes`. That map is keyed by the
//!    primary-key identity of a row that already exists; drafts are
//!    converted to `insert_marked` row changes only at the wire boundary.
//! 2. A draft transition **resets the staged plan triad**: a changed draft
//!    invalidates the previewed plan (the plan fingerprint covers the draft
//!    content), so `preview_plan`, `pending_status` and `error` go back to
//!    idle with every mutation, exactly as the React `Partial`s do.
//!
//! # Module placement
//!
//! The React file pairs with `enumCandidates.ts` in one task; the port
//! lands as two modules (`table_data_draft_inserts`,
//! `table_data_enum_candidates`) because the sources are two files and the
//! 800-line ceiling prefers small modules over one combined file.
//!
//! # Deliberate differences from React
//!
//! * **The transitions mutate instead of returning a `Partial`.** React
//!   returns `Partial<TableState>` for immutable updates; Rust has no
//!   structural sharing to preserve, so the transition applies to `&mut
//!   TableState` and returns `bool` — `false` is React's empty `Partial`
//!   (the draft was missing, the row list was empty): nothing changed.
//! * **`Map` becomes `HashMap`.** `draft_inserts` keying is by draft id and
//!   every order-sensitive read goes through `draft_insert_order`, so the
//!   hash order never leaks (the React source takes the same precaution:
//!   `orderedDrafts` reads the order array, never the `Map` iteration).
//! * **`cloneDraftValues` is `Clone`.** The React hand-rolled recursion is
//!   a JSON deep copy; `serde_json::Value`'s `Clone` is the same deep copy,
//!   so duplicating a draft never shares mutable cells.
//! * **`Value | null | undefined` splits into `Option<&Value>`.** `None` is
//!   React's `undefined` ("remove the column — do not write it"); a JSON
//!   `null` is `Some(Value::Null)` and writes SQL `NULL`. The two
//!   statements stay different, exactly as the source documents.
//! * **`Math.random().toString(36).slice(2, 8)` is a caller-supplied
//!   suffix.** The counter half of [`create_draft_id`] is deterministic
//!   and testable; the random half is a host concern, so the function takes
//!   the suffix instead of reaching for a global RNG. The module-level
//!   counter singleton of React becomes [`next_draft_id`] on the same
//!   global-`AtomicU64` reasoning (a draft id must survive paging,
//!   sorting and filtering, so it may not derive from a row index).
//! * **The slice layer is not ported.** `DraftStoreSlice` /
//!   `createDraftSlice` are store wiring (patch/read closures); the Rune
//!   host owns the `TableState` and calls these transitions directly, the
//!   same split the T089 / T127 / T210 ports took.

#![allow(dead_code)] // wired by the table-data store track

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;

use crate::table_data_connection_state::{PendingInsert, PendingStatus, TableState};

/// The module-level `draftCounter` singleton of the React source.
static DRAFT_COUNTER: AtomicU64 = AtomicU64::new(0);

/// `draftCounter.toString(36)` — the counter half of a draft id, in
/// lowercase base 36 (digits then `a..z`), as JS `Number.toString(36)`
/// prints it.
fn base36(counter: u64) -> String {
    if counter == 0 {
        return "0".to_owned();
    }
    let digits = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut out = Vec::new();
    let mut n = counter;
    while n > 0 {
        out.push(digits[(n % 36) as usize]);
        n /= 36;
    }
    out.reverse();
    String::from_utf8(out).expect("base36 digits are ASCII")
}

/// `createDraftId`: `draft-<counter.toString(36)>-<random>`. The caller
/// supplies the random suffix (six base-36 characters in practice); the
/// counter comes from the same global sequence the React module kept.
pub(crate) fn create_draft_id(counter: u64, random_suffix: &str) -> String {
    format!("draft-{}-{}", base36(counter), random_suffix)
}

/// The whole `createDraftId` call: bumps the global counter and pairs it
/// with the caller-supplied random suffix. Unique for the process, never
/// derived from a row index, so it survives paging, sorting and filtering.
pub(crate) fn next_draft_id(random_suffix: &str) -> String {
    let counter = DRAFT_COUNTER.fetch_add(1, Ordering::SeqCst) + 1;
    create_draft_id(counter, random_suffix)
}

/// Number of staged new rows (`draftInsertCount`).
pub(crate) fn draft_insert_count(ts: &TableState) -> usize {
    ts.draft_inserts.len()
}

/// The drafts in creation order (`orderedDrafts`).
///
/// The order is taken from `draft_insert_order`, never from the
/// `HashMap`'s arbitrary iteration: the insertion order decides which row
/// receives which auto-increment value, and the back end deliberately does
/// not sort the insert statements. An order entry with no draft in the map
/// is skipped (the React `if (draft)` guard).
pub(crate) fn ordered_drafts(ts: &TableState) -> Vec<&PendingInsert> {
    ts.draft_insert_order
        .iter()
        .filter_map(|draft_id| ts.draft_inserts.get(draft_id))
        .collect()
}

/// Deep copy of a draft's values (`cloneDraftValues`): duplicating a row
/// must not share mutable cells.
pub(crate) fn clone_draft_values(values: &HashMap<String, Value>) -> HashMap<String, Value> {
    values.clone()
}

/// Append an empty draft (`withNewDraft`).
///
/// An empty draft is legal: "write no column at all" is a valid insert (the
/// database fills every default), and the preview rejects it only when a
/// NOT NULL column has no default.
pub(crate) fn with_new_draft(ts: &mut TableState, draft_id: &str) -> bool {
    ts.draft_inserts.insert(
        draft_id.to_owned(),
        PendingInsert {
            draft_id: draft_id.to_owned(),
            values: HashMap::new(),
        },
    );
    ts.draft_insert_order.push(draft_id.to_owned());
    reset_plan_trio(ts);
    true
}

/// Write one cell of a draft (`withDraftValue`).
///
/// `value == None` (React `undefined`) **removes** the column: "not
/// written" is expressed by the absence of the key, never by a `null`
/// value. A JSON `null` writes SQL `NULL`. A missing draft is a no-op
/// (`false`), exactly as the React `{}` early return.
pub(crate) fn with_draft_value(
    ts: &mut TableState,
    draft_id: &str,
    column: &str,
    value: Option<&Value>,
) -> bool {
    let Some(draft) = ts.draft_inserts.get(draft_id) else {
        return false;
    };
    let mut values = draft.values.clone();
    match value {
        None => {
            values.remove(column);
        }
        Some(value) => {
            values.insert(column.to_owned(), value.clone());
        }
    }
    let draft = PendingInsert {
        draft_id: draft_id.to_owned(),
        values,
    };
    ts.draft_inserts.insert(draft_id.to_owned(), draft);
    reset_plan_trio(ts);
    true
}

/// Drop a draft row (`withoutDraft`). A missing draft is a no-op (`false`).
pub(crate) fn without_draft(ts: &mut TableState, draft_id: &str) -> bool {
    if ts.draft_inserts.remove(draft_id).is_none() {
        return false;
    }
    ts.draft_insert_order.retain(|id| id != draft_id);
    reset_plan_trio(ts);
    true
}

/// One row of a paste-as-new-rows payload (`withDraftRowsFromCopy`): the
/// caller (which already stripped primary-key / auto-increment columns)
/// decides what is written; the model stays the single truth.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DraftRowCopy {
    pub draft_id: String,
    /// A `null` value is an explicit SQL NULL, an absent key is "don't
    /// write this column".
    pub values: HashMap<String, Value>,
}

/// Append several prefilled drafts at once (`withDraftRowsFromCopy`,
/// DBR-02: paste rows as new rows).
///
/// The values follow the draft invariants verbatim; order is preserved
/// (the draft order decides auto-increment assignment, and copies land in
/// the same order they were copied in). An empty row list is a no-op
/// (`false`).
pub(crate) fn with_draft_rows_from_copy(ts: &mut TableState, rows: &[DraftRowCopy]) -> bool {
    if rows.is_empty() {
        return false;
    }
    for row in rows {
        ts.draft_inserts.insert(
            row.draft_id.clone(),
            PendingInsert {
                draft_id: row.draft_id.clone(),
                values: row.values.clone(),
            },
        );
        ts.draft_insert_order.push(row.draft_id.clone());
    }
    reset_plan_trio(ts);
    true
}

/// Copy a draft into a new one (`withDuplicatedDraft`); the copy is appended
/// at the end of the order. A missing source draft is a no-op (`false`).
pub(crate) fn with_duplicated_draft(
    ts: &mut TableState,
    draft_id: &str,
    new_draft_id: &str,
) -> bool {
    let Some(draft) = ts.draft_inserts.get(draft_id) else {
        return false;
    };
    let values = clone_draft_values(&draft.values);
    ts.draft_inserts.insert(
        new_draft_id.to_owned(),
        PendingInsert {
            draft_id: new_draft_id.to_owned(),
            values,
        },
    );
    ts.draft_insert_order.push(new_draft_id.to_owned());
    reset_plan_trio(ts);
    true
}

/// The plan triad every draft mutation resets (`preview_plan: null`,
/// `pending_status: 'idle'`, `error: null`): the fingerprint of the plan
/// covers the draft content, so the old plan no longer describes it.
fn reset_plan_trio(ts: &mut TableState) {
    ts.preview_plan = None;
    ts.pending_status = PendingStatus::Idle;
    ts.error = None;
}
