/**
 * Pure helpers for the new-row drafts (04 §3.7 / §5 Step 7).
 *
 * Everything here is a **transition on a `TableState` slice**, not an action:
 * the store owns the write, this module owns the rules. Keeping them apart is
 * what keeps `tableDataStore.ts` below the 800-line ceiling and makes the rules
 * testable without a store.
 *
 * Two invariants are enforced here and nowhere else:
 *
 * 1. A draft never touches `ts.rows`. A new row has no identity yet, and the
 *    real row set feeds identity building, de-duplication, export and sorting
 *    (04 §3.7 R-5).
 * 2. A draft never touches `ts.pendingChanges`. That map is keyed by the
 *    primary-key identity of a row that already exists; drafts are converted
 *    to `insertMarked` row changes only at the wire boundary.
 */

import type { Value } from '../../types';
import type { PendingInsert } from '../../lib/tableChanges';
import type { TableState } from './types';

/**
 * Draft ids must survive paging, sorting and filtering, so they may **not** be
 * derived from a row index. A monotonic counter plus a random suffix gives an
 * id that is unique inside the page and stable for the lifetime of the draft.
 */
let draftCounter = 0;

export function createDraftId(): string {
  draftCounter += 1;
  const random = Math.random().toString(36).slice(2, 8);
  return `draft-${draftCounter.toString(36)}-${random}`;
}

/** Number of staged new rows. */
export function draftInsertCount(ts: TableState): number {
  return ts.draftInserts.size;
}

/**
 * The drafts in creation order.
 *
 * The order is taken from `draftInsertOrder`, never from the `Map` iteration
 * order: the insertion order decides which row receives which auto-increment
 * value, and the back end deliberately does not sort the insert statements.
 */
export function orderedDrafts(ts: TableState): PendingInsert[] {
  const ordered: PendingInsert[] = [];
  for (const draftId of ts.draftInsertOrder) {
    const draft = ts.draftInserts.get(draftId);
    if (draft) ordered.push(draft);
  }
  return ordered;
}

/** Deep copy of a draft's values: duplicating a row must not share mutable cells. */
export function cloneDraftValues(
  values: Record<string, Value | null>,
): Record<string, Value | null> {
  const copy: Record<string, Value | null> = {};
  for (const [column, value] of Object.entries(values)) {
    copy[column] = cloneDraftValue(value);
  }
  return copy;
}

function cloneDraftValue(value: Value | null): Value | null {
  if (value === null || typeof value !== 'object') return value;
  if (Array.isArray(value)) {
    return value.map((item) => cloneDraftValue(item as Value | null)) as Value;
  }
  const source = value as Record<string, unknown>;
  const copy: Record<string, unknown> = {};
  for (const [key, item] of Object.entries(source)) {
    copy[key] = cloneDraftValue(item as Value | null);
  }
  return copy;
}

/**
 * Append an empty draft.
 *
 * An empty draft is legal: "write no column at all" is a valid insert (the
 * database fills every default), and the preview rejects it only when a
 * NOT NULL column has no default (04 §7 #2).
 */
export function withNewDraft(ts: TableState, draftId: string): Partial<TableState> {
  const draftInserts = new Map(ts.draftInserts);
  draftInserts.set(draftId, { draftId, values: {} });
  return {
    draftInserts,
    draftInsertOrder: [...ts.draftInsertOrder, draftId],
    previewPlan: null,
    pendingStatus: 'idle',
    error: null,
  };
}

/**
 * Write one cell of a draft.
 *
 * `undefined` **removes** the column: "not written" is expressed by the
 * absence of the key, never by a `null` value. `null` writes SQL `NULL`.
 */
export function withDraftValue(
  ts: TableState,
  draftId: string,
  column: string,
  value: Value | null | undefined,
): Partial<TableState> {
  const draft = ts.draftInserts.get(draftId);
  if (!draft) return {};
  const values = { ...draft.values };
  if (value === undefined) {
    delete values[column];
  } else {
    values[column] = value;
  }
  const draftInserts = new Map(ts.draftInserts);
  draftInserts.set(draftId, { ...draft, values });
  return {
    draftInserts,
    // A changed draft invalidates the previewed plan: the fingerprint of the
    // plan covers the draft content, so the old plan no longer describes it.
    previewPlan: null,
    pendingStatus: 'idle',
    error: null,
  };
}

/** Drop a draft row. */
export function withoutDraft(ts: TableState, draftId: string): Partial<TableState> {
  if (!ts.draftInserts.has(draftId)) return {};
  const draftInserts = new Map(ts.draftInserts);
  draftInserts.delete(draftId);
  return {
    draftInserts,
    draftInsertOrder: ts.draftInsertOrder.filter((id) => id !== draftId),
    previewPlan: null,
    pendingStatus: 'idle',
    error: null,
  };
}

/** Copy a draft into a new one; the copy is appended at the end of the order. */
export function withDuplicatedDraft(
  ts: TableState,
  draftId: string,
  newDraftId: string,
): Partial<TableState> {
  const draft = ts.draftInserts.get(draftId);
  if (!draft) return {};
  const draftInserts = new Map(ts.draftInserts);
  draftInserts.set(newDraftId, { draftId: newDraftId, values: cloneDraftValues(draft.values) });
  return {
    draftInserts,
    draftInsertOrder: [...ts.draftInsertOrder, newDraftId],
    previewPlan: null,
    pendingStatus: 'idle',
    error: null,
  };
}

/**
 * The draft actions of the store, kept as a slice so `tableDataStore.ts` only
 * carries the wiring (the same pattern as the selection slice).
 */
export interface DraftStoreSlice {
  /** Append an empty new-row draft; returns the new draft id. */
  addDraftRow: (panelId: string) => string | null;
  /** Write one cell of a draft; `undefined` removes the column (do not write it). */
  setDraftValue: (
    panelId: string,
    draftId: string,
    column: string,
    value: Value | null | undefined,
  ) => void;
  /** Drop a draft row. */
  removeDraftRow: (panelId: string, draftId: string) => void;
  /** Copy a draft row into a new one; returns the new draft id. */
  duplicateDraftRow: (panelId: string, draftId: string) => string | null;
}

export function createDraftSlice(
  patch: (panelId: string, updater: (ts: TableState) => Partial<TableState>) => void,
  read: (panelId: string) => TableState | undefined,
): DraftStoreSlice {
  return {
    addDraftRow: (panelId) => {
      if (!read(panelId)) return null;
      const draftId = createDraftId();
      patch(panelId, (ts) => withNewDraft(ts, draftId));
      return draftId;
    },
    setDraftValue: (panelId, draftId, column, value) =>
      patch(panelId, (ts) => withDraftValue(ts, draftId, column, value)),
    removeDraftRow: (panelId, draftId) => patch(panelId, (ts) => withoutDraft(ts, draftId)),
    duplicateDraftRow: (panelId, draftId) => {
      if (!read(panelId)?.draftInserts.has(draftId)) return null;
      const newDraftId = createDraftId();
      patch(panelId, (ts) => withDuplicatedDraft(ts, draftId, newDraftId));
      return newDraftId;
    },
  };
}
