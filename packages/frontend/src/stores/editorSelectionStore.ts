import { create } from 'zustand';

/**
 * The current text selection of every mounted query editor, keyed by panel id.
 *
 * The selection is ephemeral: it exists only while at least one editor for the
 * panel is mounted, and `SqlEditor` reports it on every `selectionSet` /
 * `docChanged` update. The AI drawer reads it for the active query panel so a
 * send can freeze the selection into the chat snapshot.
 */
interface EditorSelectionState {
  byPanel: Record<string, string>;
  setSelection: (panelId: string, selection: string) => void;
  clearSelection: (panelId: string) => void;
}

export const useEditorSelectionStore = create<EditorSelectionState>((set) => ({
  byPanel: {},
  setSelection: (panelId, selection) =>
    set((state) => {
      if (!selection) {
        if (!(panelId in state.byPanel)) return state;
        const byPanel = { ...state.byPanel };
        delete byPanel[panelId];
        return { byPanel };
      }
      if (state.byPanel[panelId] === selection) return state;
      return { byPanel: { ...state.byPanel, [panelId]: selection } };
    }),
  clearSelection: (panelId) =>
    set((state) => {
      if (!(panelId in state.byPanel)) return state;
      const byPanel = { ...state.byPanel };
      delete byPanel[panelId];
      return { byPanel };
    }),
}));

/** Read the selection of the active query panel without subscribing. */
export function getEditorSelection(panelId: string | null | undefined): string {
  if (!panelId) return '';
  return useEditorSelectionStore.getState().byPanel[panelId] ?? '';
}

/** Imperative write used by the editor's update listener. */
export function setEditorSelection(panelId: string, selection: string): void {
  useEditorSelectionStore.getState().setSelection(panelId, selection);
}

/** Drop a panel's selection when its editor unmounts. */
export function clearEditorSelection(panelId: string): void {
  useEditorSelectionStore.getState().clearSelection(panelId);
}
