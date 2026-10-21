import { useCallback, useMemo, useState } from 'react';
import { useTableDataStore } from '../../stores/tableDataStore';
import { orderedDrafts } from '../../stores/tableData/draftInserts';
import type { TableState } from '../../stores/tableData/types';
import type { DraftRowViewModel } from '../../components/DataTable/VirtualBody';
import type { Value } from '../../types';

/**
 * The new-row draft wiring of one table panel.
 *
 * The drafts themselves live in the store (they are staged work that has to
 * survive a re-render and be committed); what lives here is the **UI state**
 * around them: which draft row the pointer is on and which draft cell is being
 * edited. Those two are not table state — they describe the current
 * interaction, not the staged work — so they stay next to the grid.
 */
export interface TableDraftRows {
  /** The drafts as the grid renders them, in creation order. */
  draftRows: DraftRowViewModel[];
  /** The draft cell that is being edited. */
  editingDraft: { draftId: string; column: string } | null;
  /** The draft row the pointer is on — the target of duplicate / discard. */
  activeDraftId: string | null;
  /** Append an empty draft row. */
  addDraftRow: () => string | null;
  /** Write one cell of a draft; `undefined` clears the column (do not write it). */
  setDraftValue: (draftId: string, column: string, value: Value | null | undefined) => void;
  /** Start editing a draft cell. */
  startDraftEdit: (draftId: string, column: string) => void;
  /** Leave the draft cell editor without writing anything. */
  cancelDraftEdit: () => void;
  /** Drop a draft row. */
  removeDraftRow: (draftId: string) => void;
  /** Copy a draft row into a new one. */
  duplicateDraftRow: (draftId: string | null) => void;
  /** The draft row became the active one (pointer / keyboard focus). */
  activateDraftRow: (draftId: string) => void;
  /**
   * Drop every draft that has no value (the Escape shortcut).
   *
   * Only the **empty** drafts go: a row the user already filled must never be
   * thrown away by a key press — it has to be discarded explicitly (04 §4.1).
   */
  removeEmptyDraftRows: () => void;
}

export function useTableDraftRows(
  panelId: string,
  ts: TableState | undefined,
  enabled: boolean,
): TableDraftRows {
  const [activeDraftId, setActiveDraftId] = useState<string | null>(null);
  const [editingDraft, setEditingDraft] = useState<{ draftId: string; column: string } | null>(
    null,
  );

  const draftRows = useMemo< DraftRowViewModel[]>(() => {
    if (!ts) return [];
    return orderedDrafts(ts).map((draft) => ({ draftId: draft.draftId, cells: draft.values }));
  }, [ts]);

  const addDraftRow = useCallback(() => {
    // A read-only panel has no insert capability at all, so the entry point is
    // hidden rather than rendered and rejected afterwards (04 §8, R4).
    if (!enabled) return null;
    const draftId = useTableDataStore.getState().addDraftRow(panelId);
    if (draftId) setActiveDraftId(draftId);
    return draftId;
  }, [enabled, panelId]);

  const setDraftValue = useCallback(
    (draftId: string, column: string, value: Value | null | undefined) => {
      useTableDataStore.getState().setDraftValue(panelId, draftId, column, value);
      setEditingDraft(null);
    },
    [panelId],
  );

  const startDraftEdit = useCallback((draftId: string, column: string) => {
    setActiveDraftId(draftId);
    setEditingDraft({ draftId, column });
  }, []);

  const cancelDraftEdit = useCallback(() => setEditingDraft(null), []);

  const removeDraftRow = useCallback(
    (draftId: string) => {
      useTableDataStore.getState().removeDraftRow(panelId, draftId);
      setActiveDraftId((current) => (current === draftId ? null : current));
      setEditingDraft((current) => (current?.draftId === draftId ? null : current));
    },
    [panelId],
  );

  const duplicateDraftRow = useCallback(
    (draftId: string | null) => {
      if (!draftId || !enabled) return;
      const newDraftId = useTableDataStore.getState().duplicateDraftRow(panelId, draftId);
      if (newDraftId) setActiveDraftId(newDraftId);
    },
    [enabled, panelId],
  );

  const activateDraftRow = useCallback((draftId: string) => setActiveDraftId(draftId), []);

  const removeEmptyDraftRows = useCallback(() => {
    const state = useTableDataStore.getState().byPanel.get(panelId);
    if (!state) return;
    for (const draft of orderedDrafts(state)) {
      if (Object.keys(draft.values).length > 0) continue;
      useTableDataStore.getState().removeDraftRow(panelId, draft.draftId);
      setActiveDraftId((current) => (current === draft.draftId ? null : current));
    }
  }, [panelId]);

  return {
    draftRows,
    editingDraft,
    activeDraftId,
    addDraftRow,
    setDraftValue,
    startDraftEdit,
    cancelDraftEdit,
    removeDraftRow,
    duplicateDraftRow,
    activateDraftRow,
    removeEmptyDraftRows,
  };
}
