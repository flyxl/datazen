import { create } from 'zustand';

export type DialogId = 'new-connection' | null;

export type ConnectionsViewMode = 'grid' | 'list';

export type WorkspaceSidebarMode = 'icons' | 'expanded';

const WORKSPACE_SIDEBAR_MODE_KEY = 'datazen:workspace-sidebar-mode';

function loadWorkspaceSidebarMode(): WorkspaceSidebarMode {
  if (typeof window === 'undefined' || !window.localStorage) return 'icons';
  const stored = localStorage.getItem(WORKSPACE_SIDEBAR_MODE_KEY);
  return stored === 'expanded' ? 'expanded' : 'icons';
}

function persistWorkspaceSidebarMode(mode: WorkspaceSidebarMode): void {
  if (typeof window === 'undefined' || !window.localStorage) return;
  try {
    localStorage.setItem(WORKSPACE_SIDEBAR_MODE_KEY, mode);
  } catch {
    // best-effort
  }
}

interface UiStore {
  mainSidebarWidth: number;
  connectionSidebarWidth: number;
  editorHeight: number;
  resultHeight: number;
  connectionsViewMode: ConnectionsViewMode;
  activeDialog: DialogId;
  isFullscreen: boolean;
  workspaceSidebarMode: WorkspaceSidebarMode;

  setMainSidebarWidth: (w: number) => void;
  setConnectionSidebarWidth: (w: number) => void;
  setEditorHeight: (h: number) => void;
  setResultHeight: (h: number) => void;
  openDialog: (id: Exclude<DialogId, null>) => void;
  closeDialog: () => void;
  setConnectionsViewMode: (mode: ConnectionsViewMode) => void;
  setFullscreen: (value: boolean) => void;
  syncFullscreen: () => Promise<boolean>;
  toggleWorkspaceSidebarMode: () => void;
  setWorkspaceSidebarMode: (mode: WorkspaceSidebarMode) => void;
}

export const useUiStore = create<UiStore>((set, get) => ({
  mainSidebarWidth: 220,
  connectionSidebarWidth: 280,
  editorHeight: 320,
  resultHeight: 360,
  connectionsViewMode: 'grid',
  activeDialog: null,
  isFullscreen: false,
  workspaceSidebarMode: loadWorkspaceSidebarMode(),

  setMainSidebarWidth: (w) => set({ mainSidebarWidth: w }),
  setConnectionSidebarWidth: (w) => set({ connectionSidebarWidth: w }),
  setEditorHeight: (h) => set({ editorHeight: h }),
  setResultHeight: (h) => set({ resultHeight: h }),
  openDialog: (id) => set({ activeDialog: id }),
  closeDialog: () => set({ activeDialog: null }),
  setConnectionsViewMode: (connectionsViewMode) => set({ connectionsViewMode }),
  setFullscreen: (isFullscreen) => set({ isFullscreen }),
  toggleWorkspaceSidebarMode: () => {
    const next = get().workspaceSidebarMode === 'icons' ? 'expanded' : 'icons';
    persistWorkspaceSidebarMode(next);
    set({ workspaceSidebarMode: next });
  },
  setWorkspaceSidebarMode: (mode) => {
    persistWorkspaceSidebarMode(mode);
    set({ workspaceSidebarMode: mode });
  },
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
