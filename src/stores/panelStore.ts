import { create } from 'zustand';
import type { DatabaseType } from '../types';

// ── Sub-tab types ────────────────────────────────────────────────

export type SubTabId = 'data' | 'structure' | 'indexes' | 'foreignKeys' | 'ddl';

// ── Panel types ──────────────────────────────────────────────────

interface PanelBase {
  id: string;
  /** Persistent connection config ID. */
  configId: string;
  /** Live connection ID (from activeConnectionStore). */
  connectionId: string;
  connectionName: string;
  databaseType: DatabaseType;
}

export interface TablePanel extends PanelBase {
  type: 'table';
  tableName: string;
  subTab: SubTabId;
  structureEditing?: boolean;
}

export interface ViewPanel extends PanelBase {
  type: 'view';
  viewName: string;
  subTab: SubTabId;
}

export interface QueryPanel extends PanelBase {
  type: 'query';
  queryTabId: string;
  title: string;
}

export interface CreateTablePanel extends PanelBase {
  type: 'create-table';
}

export interface ErDiagramPanel extends PanelBase {
  type: 'er-diagram';
  focusTable?: string;
}

export interface ObjectsPanel extends PanelBase {
  type: 'objects';
}

export interface PrivilegesPanel extends PanelBase {
  type: 'privileges';
}

export interface DatabaseObjectPanel extends PanelBase {
  type: 'db-object';
  objectKind: 'function' | 'procedure' | 'trigger' | 'sequence' | 'type';
  objectName: string;
  objectSchema?: string;
}

export interface RedisDbPanel extends PanelBase {
  type: 'redis-db';
  dbName: string;
}

export type Panel =
  | TablePanel
  | ViewPanel
  | QueryPanel
  | CreateTablePanel
  | ErDiagramPanel
  | ObjectsPanel
  | PrivilegesPanel
  | DatabaseObjectPanel
  | RedisDbPanel;

// ── ID generation ────────────────────────────────────────────────

let counter = 0;
export function nextPanelId(prefix: string): string {
  counter += 1;
  return `panel-${prefix}-${counter}`;
}

// ── Connection context (for panel creation helpers) ──────────────

export interface ConnectionContext {
  configId: string;
  connectionId: string;
  connectionName: string;
  databaseType: DatabaseType;
}

// ── Store ────────────────────────────────────────────────────────

interface PanelState {
  panels: Panel[];
  activePanelId: string | null;
}

interface PanelActions {
  /** Add a panel and optionally activate it. */
  addPanel: (panel: Panel, activate?: boolean) => void;
  /** Remove a panel by ID. Adjusts active panel if needed. */
  removePanel: (panelId: string) => void;
  /** Remove all panels for a specific connection. */
  removeAllForConnection: (configId: string) => void;
  /** Activate a panel by ID. */
  setActivePanel: (panelId: string) => void;
  /** Update a panel (partial merge). */
  updatePanel: (panelId: string, patch: Partial<Panel>) => void;
  /** Close all panels except the given one. */
  closeOtherPanels: (panelId: string) => void;
  /** Close all panels. */
  closeAllPanels: () => void;
  /** Close panels to the right of the given one. */
  closePanelsToTheRight: (panelId: string) => void;
  /** Close panels to the left of the given one. */
  closePanelsToTheLeft: (panelId: string) => void;
}

function resolveNextActive(
  panels: Panel[],
  removedId: string,
  currentActiveId: string | null,
): string | null {
  if (currentActiveId !== removedId) return currentActiveId;
  const idx = panels.findIndex((p) => p.id === removedId);
  if (idx < 0) return null;
  const remaining = panels.filter((p) => p.id !== removedId);
  if (remaining.length === 0) return null;
  return remaining[Math.min(idx, remaining.length - 1)].id;
}

export const usePanelStore = create<PanelState & PanelActions>((set, get) => ({
  panels: [],
  activePanelId: null,

  addPanel: (panel, activate = true) => {
    set((s) => ({
      panels: [...s.panels, panel],
      activePanelId: activate ? panel.id : s.activePanelId,
    }));
  },

  removePanel: (panelId) => {
    const { panels, activePanelId } = get();
    const nextActive = resolveNextActive(panels, panelId, activePanelId);
    set({
      panels: panels.filter((p) => p.id !== panelId),
      activePanelId: nextActive,
    });
  },

  removeAllForConnection: (configId) => {
    const { panels, activePanelId } = get();
    const remaining = panels.filter((p) => p.configId !== configId);
    const activeStillExists = remaining.some((p) => p.id === activePanelId);
    set({
      panels: remaining,
      activePanelId: activeStillExists
        ? activePanelId
        : (remaining[remaining.length - 1]?.id ?? null),
    });
  },

  setActivePanel: (panelId) => {
    set({ activePanelId: panelId });
  },

  updatePanel: (panelId, patch) => {
    set((s) => ({
      panels: s.panels.map((p) => (p.id === panelId ? ({ ...p, ...patch } as Panel) : p)),
    }));
  },

  closeOtherPanels: (panelId) => {
    set((s) => ({
      panels: s.panels.filter((p) => p.id === panelId),
      activePanelId: panelId,
    }));
  },

  closeAllPanels: () => {
    set({ panels: [], activePanelId: null });
  },

  closePanelsToTheRight: (panelId) => {
    set((s) => {
      const idx = s.panels.findIndex((p) => p.id === panelId);
      if (idx < 0) return s;
      const kept = s.panels.slice(0, idx + 1);
      const activeStillExists = kept.some((p) => p.id === s.activePanelId);
      return {
        panels: kept,
        activePanelId: activeStillExists ? s.activePanelId : panelId,
      };
    });
  },

  closePanelsToTheLeft: (panelId) => {
    set((s) => {
      const idx = s.panels.findIndex((p) => p.id === panelId);
      if (idx < 0) return s;
      const kept = s.panels.slice(idx);
      const activeStillExists = kept.some((p) => p.id === s.activePanelId);
      return {
        panels: kept,
        activePanelId: activeStillExists ? s.activePanelId : panelId,
      };
    });
  },
}));
