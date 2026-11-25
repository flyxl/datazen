//! Pure logic of `windows/connection/useTableDraftRows.ts` (and the draft
//! rules from `stores/tableData/draftInserts.ts`).
//!
//! The React hook keeps only two pieces of UI state (`activeDraftId`,
//! `editingDraft`) next to the grid; the drafts themselves live in the store.
//! Here the whole snapshot is host-owned: [`DraftRowsVm`] holds the drafts in
//! creation order plus the two interaction fields and an insert capability.
//! Every operation is a pure transition on that VM — no store, IPC, timer or
//! random id generator (the host supplies ids).
//!
//! Two invariants are enforced here and nowhere else: a draft never touches
//! real rows, and "not written" is an absent column key (an explicit `null`
//! writes SQL NULL). Duplicating a draft deep-copies its cells.
//!
//! ## Removal stage
//!
//! No production host mounts the grid yet; every reusable item carries a
//! per-item `#[allow(dead_code)]` with removal stage "drop when the table host
//! owns draft rows".

use std::collections::BTreeMap;

use serde_json::Value;

/// One insert draft: a stable id and its written columns.
#[allow(dead_code)] // VM mirror; remove when the host owns drafts.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DraftRowVm {
    pub id: String,
    /// Column -> value. `Null` writes SQL NULL; an absent key writes nothing.
    pub values: BTreeMap<String, Value>,
}

/// The host-owned draft snapshot plus the hook's two UI-state fields.
#[allow(dead_code)] // VM surface; remove when the host wires draft rows.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DraftRowsVm {
    /// Drafts in creation order (never derived from a row index).
    pub rows: Vec<DraftRowVm>,
    pub active_draft_id: Option<String>,
    /// The cell being edited: (draft id, column).
    pub editing_draft: Option<(String, String)>,
    /// React's `enabled`: a read-only panel has no insert entry point at all.
    pub enabled: bool,
    /// A mutating draft operation invalidates the previously previewed plan.
    pub preview_stale: bool,
}

#[allow(dead_code)] // pure helpers; remove with the host.
impl DraftRowsVm {
    /// React `draftInsertCount` / `orderedDrafts` (rows are already in order).
    pub fn draft_insert_count(&self) -> usize {
        self.rows.len()
    }

    pub fn ordered_drafts(&self) -> &[DraftRowVm] {
        &self.rows
    }

    /// React `enabled` guard: a read-only panel cannot append a draft.
    pub fn can_add(&self) -> bool {
        self.enabled
    }

    /// React `hasPendingWork` over drafts only.
    pub fn has_draft_work(&self) -> bool {
        !self.rows.is_empty()
    }

    /// React's three-state distinction: a draft is empty when it writes no
    /// column at all ("all defaults"), not when its columns are all NULL.
    pub fn is_empty_draft(draft: &DraftRowVm) -> bool {
        draft.values.is_empty()
    }

    /// Append an empty draft with a host-supplied id. Returns the id, or
    /// `None` when the panel is read-only (React returns `null`).
    pub fn add_draft_row(&mut self, draft_id: String) -> Option<String> {
        if !self.enabled {
            return None;
        }
        self.rows.push(DraftRowVm {
            id: draft_id.clone(),
            values: BTreeMap::new(),
        });
        self.active_draft_id = Some(draft_id.clone());
        self.preview_stale = true;
        Some(draft_id)
    }

    /// Write one cell. `Some(value)` writes (including `Value::Null`);
    /// `None` removes the column key entirely. React clears `editingDraft`
    /// after every write, successful or not.
    pub fn set_draft_value(
        &mut self,
        draft_id: &str,
        column: &str,
        value: Option<Value>,
    ) -> bool {
        let Some(draft) = self.rows.iter_mut().find(|draft| draft.id == draft_id) else {
            self.editing_draft = None;
            return false;
        };
        match value {
            Some(value) => {
                draft.values.insert(column.to_owned(), value);
            }
            None => {
                draft.values.remove(column);
            }
        }
        self.editing_draft = None;
        self.preview_stale = true;
        true
    }

    /// React `startDraftEdit`: focus the row and open the named cell editor.
    pub fn start_draft_edit(&mut self, draft_id: String, column: String) {
        self.active_draft_id = Some(draft_id.clone());
        self.editing_draft = Some((draft_id, column));
    }

    /// React `cancelDraftEdit`: leave the editor without writing anything.
    pub fn cancel_draft_edit(&mut self) {
        self.editing_draft = None;
    }

    /// React `removeDraftRow`: drop the row and clear the interaction fields
    /// that pointed at it, leaving unrelated active/editing drafts untouched.
    pub fn remove_draft_row(&mut self, draft_id: &str) -> bool {
        // React applies the clearing setters regardless of whether the store
        // row existed: a stale active/editing id must not survive a removal.
        if self.active_draft_id.as_deref() == Some(draft_id) {
            self.active_draft_id = None;
        }
        if self
            .editing_draft
            .as_ref()
            .is_some_and(|(id, _)| id == draft_id)
        {
            self.editing_draft = None;
        }
        let before = self.rows.len();
        self.rows.retain(|draft| draft.id != draft_id);
        if self.rows.len() == before {
            return false;
        }
        self.preview_stale = true;
        true
    }

    /// React `duplicateDraftRow`: deep-copy the cells into a new draft appended
    /// at the end of the creation order. A read-only panel cannot duplicate.
    pub fn duplicate_draft_row(&mut self, draft_id: &str, new_id: String) -> bool {
        if !self.enabled {
            return false;
        }
        let Some(source) = self.rows.iter().find(|draft| draft.id == draft_id).cloned() else {
            return false;
        };
        self.rows.push(DraftRowVm {
            id: new_id.clone(),
            values: source.values,
        });
        self.active_draft_id = Some(new_id);
        self.preview_stale = true;
        true
    }

    /// React `activateDraftRow` (pointer / keyboard focus).
    pub fn activate_draft_row(&mut self, draft_id: String) {
        self.active_draft_id = Some(draft_id);
    }

    /// React `removeEmptyDraftRows` (Escape): drop only drafts that write no
    /// column. Filled drafts are never discarded by a key press.
    pub fn remove_empty_draft_rows(&mut self) -> usize {
        let empty_ids: Vec<String> = self
            .rows
            .iter()
            .filter(|draft| Self::is_empty_draft(draft))
            .map(|draft| draft.id.clone())
            .collect();
        let mut removed = 0;
        for id in empty_ids {
            // React's Escape handler removes the row and clears only a matching
            // activeDraftId; it never touches setEditingDraft. Do not delegate
            // to remove_draft_row (which would clear editing too).
            let before = self.rows.len();
            self.rows.retain(|draft| draft.id != id);
            if self.rows.len() == before {
                continue;
            }
            if self.active_draft_id.as_deref() == Some(&id) {
                self.active_draft_id = None;
            }
            self.preview_stale = true;
            removed += 1;
        }
        removed
    }

    /// React `withDraftRowsFromCopy` (DBR-02): append prefilled drafts in copy
    /// order. `Null` is explicit NULL, an absent key writes nothing.
    pub fn add_draft_rows_from_copy(&mut self, rows: &[(String, BTreeMap<String, Value>)]) -> Vec<String> {
        // React `withDraftRowsFromCopy` guards only on rows.length === 0: a
        // read-only panel may still paste prefilled rows as drafts.
        if rows.is_empty() {
            return Vec::new();
        }
        let mut ids = Vec::new();
        for (id, values) in rows {
            self.rows.push(DraftRowVm {
                id: id.clone(),
                values: values.clone(),
            });
            ids.push(id.clone());
        }
        self.preview_stale = true;
        ids
    }
}

/// The wire shape of one draft (`draftsForWire`): an insert-marked change with
/// an empty identity, sorted changed columns and the written values.
#[allow(dead_code)] // pure result; remove when the host builds change requests.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DraftWire {
    pub insert_marked: bool,
    pub changed_columns: Vec<String>,
    pub current_values: BTreeMap<String, Value>,
    pub delete_marked: bool,
}

/// React `draftsForWire` entry: changed columns are sorted; explicit NULL stays
/// in `current_values` and a missing key never becomes a NULL entry.
#[allow(dead_code)] // pure helper; remove when the wire port lands.
pub fn draft_wire(draft: &DraftRowVm) -> DraftWire {
    DraftWire {
        insert_marked: true,
        changed_columns: draft.values.keys().cloned().collect(),
        current_values: draft.values.clone(),
        delete_marked: false,
    }
}
