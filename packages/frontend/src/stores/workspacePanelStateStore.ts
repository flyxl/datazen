import { create } from 'zustand';
import type { WorkflowPanel } from '../windows/workflow/workflowPanelGuards';
import type { DashboardWidget } from '../types/dashboard';
import type { WorkflowListItem } from '../types';

/**
 * Per-mode view-state snapshots for the main-shell workspace panels.
 *
 * The workspace modes (`workflow` / `dashboard` / `workspace` / `extension`)
 * render conditionally — only the active mode keeps its DOM tree mounted.
 * When the user switches away and back, each panel restores its previous
 * view state from here instead of starting over.
 *
 * Only recreatable view state lives here (open tabs, drafts, editor/dialog
 * UI). Server-backed data (workflow list, dashboard boards, wapp list) stays
 * in its own store / reloads on mount. Pure one-shot confirmations and
 * transient error banners are intentionally NOT snapshotted.
 */
export interface WorkflowWorkspaceSnapshot {
  panels: WorkflowPanel[];
  activePanelId: string | null;
  activeStepIndex: number | null;
  sideTab: 'workflows' | 'history';
  variables: Record<string, string>;
}

export interface DashboardWorkspaceSnapshot {
  activeDashboardId: string;
  editorOpen: boolean;
  editingWidget: DashboardWidget | null;
  isNewWidget: boolean;
  editorHiddenSql?: { connectionId: string; sql: string };
  userWorkflows: WorkflowListItem[];
  historyOpen: boolean;
  historyWidget: DashboardWidget | null;
  renaming: boolean;
  nameDraft: string;
}

export type ExtensionFilter = 'all' | 'workspace' | 'theme';

export interface ExtensionWorkspaceSnapshot {
  search: string;
  filter: ExtensionFilter;
}

interface WorkspacePanelStateStore {
  workflow: WorkflowWorkspaceSnapshot | null;
  dashboard: DashboardWorkspaceSnapshot | null;
  extension: ExtensionWorkspaceSnapshot | null;
  saveWorkflowSnapshot: (snapshot: WorkflowWorkspaceSnapshot) => void;
  saveDashboardSnapshot: (snapshot: DashboardWorkspaceSnapshot) => void;
  saveExtensionSnapshot: (snapshot: ExtensionWorkspaceSnapshot) => void;
  /** Drops the dashboard snapshot so an explicit open request shows the requested board. */
  clearDashboardSnapshot: () => void;
  clearWorkspacePanelSnapshots: () => void;
}

export const useWorkspacePanelStateStore = create<WorkspacePanelStateStore>((set) => ({
  workflow: null,
  dashboard: null,
  extension: null,
  saveWorkflowSnapshot: (workflow) => set({ workflow }),
  saveDashboardSnapshot: (dashboard) => set({ dashboard }),
  saveExtensionSnapshot: (extension) => set({ extension }),
  clearDashboardSnapshot: () => set({ dashboard: null }),
  clearWorkspacePanelSnapshots: () => set({ workflow: null, dashboard: null, extension: null }),
}));
