import { create } from 'zustand';
import type {
  ConnectionImportSource,
  ConnectionShareMode,
} from '../components/connection/ConnectionShareDialog';

interface ConnectionShareState {
  open: boolean;
  mode: ConnectionShareMode;
  importSource: ConnectionImportSource;
  openConnectionShareDialog: (
    mode: ConnectionShareMode,
    importSource?: ConnectionImportSource,
  ) => void;
  closeConnectionShareDialog: () => void;
}

export const useConnectionShareStore = create<ConnectionShareState>((set) => ({
  open: false,
  mode: 'export',
  importSource: 'file',
  openConnectionShareDialog: (mode, importSource = 'file') =>
    set({ open: true, mode, importSource }),
  closeConnectionShareDialog: () => set({ open: false }),
}));

/** Open the in-app connection import/export dialog (main window). */
export function openConnectionShareDialog(
  mode: ConnectionShareMode,
  importSource: ConnectionImportSource = 'file',
) {
  useConnectionShareStore.getState().openConnectionShareDialog(mode, importSource);
}

/** Close the connection share dialog. */
export function closeConnectionShareDialog() {
  useConnectionShareStore.getState().closeConnectionShareDialog();
}
