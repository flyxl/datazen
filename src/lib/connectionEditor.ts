import { create } from 'zustand';

interface ConnectionEditorState {
  open: boolean;
  editId: string | null;
  openNewConnectionDialog: (editId?: string) => void;
  closeNewConnectionDialog: () => void;
}

export const useConnectionEditorStore = create<ConnectionEditorState>((set) => ({
  open: false,
  editId: null,
  openNewConnectionDialog: (editId) => set({ open: true, editId: editId ?? null }),
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
