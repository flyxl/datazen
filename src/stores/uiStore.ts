import { create } from 'zustand';

export type DialogId = 'new-connection' | null;

export type ConnectionsViewMode = 'grid' | 'list';

interface UiStore {
  mainSidebarWidth: number;
  connectionSidebarWidth: number;
  editorHeight: number;
  resultHeight: number;
  connectionsViewMode: ConnectionsViewMode;
  activeDialog: DialogId;
  isFullscreen: boolean;

  setMainSidebarWidth: (w: number) => void;
  setConnectionSidebarWidth: (w: number) => void;
  setEditorHeight: (h: number) => void;
  setResultHeight: (h: number) => void;
  openDialog: (id: Exclude<DialogId, null>) => void;
  closeDialog: () => void;
  setConnectionsViewMode: (mode: ConnectionsViewMode) => void;
  setFullscreen: (value: boolean) => void;
  syncFullscreen: () => Promise<boolean>;
}

export const useUiStore = create<UiStore>((set) => ({
  mainSidebarWidth: 220,
  connectionSidebarWidth: 280,
  editorHeight: 320,
  resultHeight: 360,
  connectionsViewMode: 'grid',
  activeDialog: null,
  isFullscreen: false,

  setMainSidebarWidth: (w) => set({ mainSidebarWidth: w }),
  setConnectionSidebarWidth: (w) => set({ connectionSidebarWidth: w }),
  setEditorHeight: (h) => set({ editorHeight: h }),
  setResultHeight: (h) => set({ resultHeight: h }),
  openDialog: (id) => set({ activeDialog: id }),
  closeDialog: () => set({ activeDialog: null }),
  setConnectionsViewMode: (connectionsViewMode) => set({ connectionsViewMode }),
  setFullscreen: (isFullscreen) => set({ isFullscreen }),
  syncFullscreen: async () => {
    if (typeof window === 'undefined' || !('__TAURI_INTERNALS__' in window)) {
      return false;
    }
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      const isFs = await getCurrentWindow().isFullscreen();
      set({ isFullscreen: isFs });
      return isFs;
    } catch {
      return false;
    }
  },
}));
