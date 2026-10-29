import { create } from 'zustand';

interface ConnectionEditorState {
  open: boolean;
  editId: string | null;
  /** Preselect group when creating a connection from a group context menu. */
  defaultGroup: string | null;
  /** Incremented on each open so create/edit dialogs remount with fresh form state. */
  openSeq: number;
  openNewConnectionDialog: (editId?: string, defaultGroup?: string) => void;
  closeNewConnectionDialog: () => void;
}

export const useConnectionEditorStore = create<ConnectionEditorState>((set) => ({
  open: false,
  editId: null,
  defaultGroup: null,
  openSeq: 0,
  openNewConnectionDialog: (editId, defaultGroup) =>
    set((state) => ({
      open: true,
      editId: editId ?? null,
      defaultGroup: editId ? null : (defaultGroup ?? null),
      openSeq: state.openSeq + 1,
    })),
  closeNewConnectionDialog: () => set({ open: false, editId: null, defaultGroup: null }),
}));

/** Open the in-app new/edit connection dialog (main window). */
export function openNewConnectionDialog(editId?: string, defaultGroup?: string) {
  useConnectionEditorStore.getState().openNewConnectionDialog(editId, defaultGroup);
}

/** Close the connection editor dialog. */
export function closeNewConnectionDialog() {
  useConnectionEditorStore.getState().closeNewConnectionDialog();
}
