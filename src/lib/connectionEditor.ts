import { create } from 'zustand';

interface ConnectionEditorState {
  open: boolean;
  editId: string | null;
  /** Incremented on each open so create/edit dialogs remount with fresh form state. */
  openSeq: number;
  openNewConnectionDialog: (editId?: string) => void;
  closeNewConnectionDialog: () => void;
}

export const useConnectionEditorStore = create<ConnectionEditorState>((set) => ({
  open: false,
  editId: null,
  openSeq: 0,
  openNewConnectionDialog: (editId) =>
    set((state) => ({
      open: true,
      editId: editId ?? null,
      openSeq: state.openSeq + 1,
    })),
  closeNewConnectionDialog: () => set({ open: false, editId: null }),
}));

/** Open the in-app new/edit connection dialog (main window). */
export function openNewConnectionDialog(editId?: string) {
  useConnectionEditorStore.getState().openNewConnectionDialog(editId);
}

/** Close the connection editor dialog. */
export function closeNewConnectionDialog() {
  useConnectionEditorStore.getState().closeNewConnectionDialog();
}
