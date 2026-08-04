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
  contextMenu: { x: number; y: number; items: { label: string; onClick: () => void }[] } | null;
  isFullscreen: boolean;

  setMainSidebarWidth: (w: number) => void;
  setConnectionSidebarWidth: (w: number) => void;
  setEditorHeight: (h: number) => void;
  setResultHeight: (h: number) => void;
  openDialog: (id: Exclude<DialogId, null>) => void;
  closeDialog: () => void;
  setContextMenu: (menu: UiStore['contextMenu']) => void;
  setConnectionsViewMode: (mode: ConnectionsViewMode) => void;
  setFullscreen: (value: boolean) => void;
}

export const useUiStore = create<UiStore>((set) => ({
  mainSidebarWidth: 220,
  connectionSidebarWidth: 280,
  editorHeight: 320,
  resultHeight: 360,
  connectionsViewMode: 'grid',
  activeDialog: null,
  contextMenu: null,
  isFullscreen: false,

  setMainSidebarWidth: (w) => set({ mainSidebarWidth: w }),
  setConnectionSidebarWidth: (w) => set({ connectionSidebarWidth: w }),
  setEditorHeight: (h) => set({ editorHeight: h }),
  setResultHeight: (h) => set({ resultHeight: h }),
  openDialog: (id) => set({ activeDialog: id }),
  closeDialog: () => set({ activeDialog: null }),
  setContextMenu: (contextMenu) => set({ contextMenu }),
  setConnectionsViewMode: (connectionsViewMode) => set({ connectionsViewMode }),
  setFullscreen: (isFullscreen) => set({ isFullscreen }),
}));

