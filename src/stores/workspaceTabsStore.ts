import { create } from 'zustand';

/** Stable workspace tab key: `${pluginId}:${pageId}`. */
export function workspaceTabKey(pluginId: string, pageId: string): string {
  return `${pluginId}:${pageId}`;
}

export interface WorkspaceTab {
  key: string;
  pluginId: string;
  pageId: string;
  title: string;
  icon?: string;
  version: string;
}

interface WorkspaceTabsStore {
  tabs: WorkspaceTab[];
  activeKey: string | null;
  /** Opens (or refocuses/updates) a tab; idempotent per `key`. */
  open: (tab: WorkspaceTab) => void;
  /**
   * Closes a tab. When the closed tab was active, activates its right
   * neighbor first, otherwise the left one; null when nothing remains.
   */
  close: (key: string) => void;
  activate: (key: string) => void;
  /** Closes every tab of a plugin (uninstall/disable). Same neighbor rule. */
  closeByPlugin: (pluginId: string) => void;
}

export const useWorkspaceTabsStore = create<WorkspaceTabsStore>((set, get) => ({
  tabs: [],
  activeKey: null,

  open: (tab) => {
    const { tabs } = get();
    const index = tabs.findIndex((t) => t.key === tab.key);
    if (index >= 0) {
      // Key conflict → idempotent: refresh metadata in place, keep order.
      const next = [...tabs];
      next[index] = tab;
      set({ tabs: next, activeKey: tab.key });
      return;
    }
    set({ tabs: [...tabs, tab], activeKey: tab.key });
  },

  close: (key) => {
    const { tabs, activeKey } = get();
    const index = tabs.findIndex((t) => t.key === key);
    if (index === -1) return;

    const next = tabs.filter((t) => t.key !== key);
    if (activeKey !== key) {
      set({ tabs: next });
      return;
    }
    // Right neighbor (now at `index`) preferred, else left, else none.
    const fallback = next[index] ?? next[index - 1] ?? null;
    set({ tabs: next, activeKey: fallback?.key ?? null });
  },

  activate: (key) => {
    if (!get().tabs.some((t) => t.key === key)) return;
    set({ activeKey: key });
  },

  closeByPlugin: (pluginId) => {
    const { tabs, activeKey } = get();
    const firstRemovedIndex = tabs.findIndex((t) => t.pluginId === pluginId);
    if (firstRemovedIndex === -1) return;

    const activeSurvives =
      activeKey !== null && tabs.some((t) => t.key === activeKey && t.pluginId !== pluginId);
    const next = tabs.filter((t) => t.pluginId !== pluginId);
    if (activeSurvives) {
      set({ tabs: next });
      return;
    }
    const fallback = next[firstRemovedIndex] ?? next[firstRemovedIndex - 1] ?? null;
    set({ tabs: next, activeKey: fallback?.key ?? null });
  },
}));
