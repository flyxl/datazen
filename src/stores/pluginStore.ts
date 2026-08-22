import { create } from 'zustand';
import { listen } from '@tauri-apps/api/event';
import { PLUGINS_CHANGED_EVENT, pluginCommands } from '../commands/plugins';
import type { PluginSummary } from '../types/plugin';

function toErrorMessage(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

interface PluginStore {
  plugins: PluginSummary[];
  loaded: boolean;
  error: string | null;
  fetch: () => Promise<void>;
  /** Optimistically flips the flag, then reconciles with an authoritative refetch. */
  setEnabled: (id: string, enabled: boolean) => Promise<void>;
  remove: (id: string) => Promise<void>;
  byId: (id: string) => PluginSummary | undefined;
}

export const usePluginStore = create<PluginStore>((set, get) => ({
  plugins: [],
  loaded: false,
  error: null,

  fetch: async () => {
    try {
      const plugins = await pluginCommands.listPlugins();
      set({ plugins, loaded: true, error: null });
    } catch (e) {
      set({ loaded: true, error: toErrorMessage(e) });
    }
  },

  setEnabled: async (id, enabled) => {
    const previous = get().plugins;
    // Optimistic flip; rolled back via refetch if the backend rejects.
    set({
      plugins: previous.map((p) => (p.id === id ? { ...p, enabled } : p)),
      error: null,
    });
    try {
      await pluginCommands.setPluginEnabled(id, enabled);
      await get().fetch();
    } catch (e) {
      // Roll back through an authoritative refetch, then surface the error
      // (fetch itself clears `error` on success, so it is re-set afterwards).
      await get().fetch();
      set({ error: toErrorMessage(e) });
      throw e;
    }
  },

  remove: async (id) => {
    try {
      await pluginCommands.removePlugin(id);
      await get().fetch();
    } catch (e) {
      set({ error: toErrorMessage(e) });
      throw e;
    }
  },

  byId: (id) => get().plugins.find((p) => p.id === id),
}));

let subscriptionStarted = false;

/**
 * Register the `plugins:changed` refresh listener exactly once per module
 * instance. The failure path resets the guard so a later retry can succeed
 * (e.g. when first attempted outside the Tauri runtime).
 */
export function ensurePluginsChangedListener(): void {
  if (subscriptionStarted) return;
  subscriptionStarted = true;
  listen(PLUGINS_CHANGED_EVENT, () => {
    void usePluginStore.getState().fetch();
  }).catch(() => {
    subscriptionStarted = false;
  });
}

ensurePluginsChangedListener();
