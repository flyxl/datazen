import { create } from 'zustand';
import { listen } from '@tauri-apps/api/event';
import { EXTENSIONS_CHANGED_EVENT, extensionCommands } from '../commands/extensions';
import type { ExtensionSummary } from '../types/extension';

function toErrorMessage(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

interface ExtensionStore {
  extensions: ExtensionSummary[];
  loaded: boolean;
  error: string | null;
  fetch: () => Promise<void>;
  /** Optimistically flips the flag, then reconciles with an authoritative refetch. */
  setEnabled: (id: string, enabled: boolean) => Promise<void>;
  remove: (id: string) => Promise<void>;
  byId: (id: string) => ExtensionSummary | undefined;
}

export const useExtensionStore = create<ExtensionStore>((set, get) => ({
  extensions: [],
  loaded: false,
  error: null,

  fetch: async () => {
    try {
      const extensions = await extensionCommands.listExtensions();
      set({ extensions, loaded: true, error: null });
    } catch (e) {
      set({ loaded: true, error: toErrorMessage(e) });
    }
  },

  setEnabled: async (id, enabled) => {
    const previous = get().extensions;
    set({
      extensions: previous.map((p) => (p.id === id ? { ...p, enabled } : p)),
      error: null,
    });
    try {
      await extensionCommands.setExtensionEnabled(id, enabled);
      await get().fetch();
    } catch (e) {
      await get().fetch();
      set({ error: toErrorMessage(e) });
      throw e;
    }
  },

  remove: async (id) => {
    try {
      await extensionCommands.removeExtension(id);
      await get().fetch();
    } catch (e) {
      set({ error: toErrorMessage(e) });
      throw e;
    }
  },

  byId: (id) => get().extensions.find((p) => p.id === id),
}));

let subscriptionStarted = false;

/**
 * Register the `plugins:changed` refresh listener exactly once per module
 * instance. The failure path resets the guard so a later retry can succeed
 * (e.g. when first attempted outside the Tauri runtime).
 */
export function ensureExtensionsChangedListener(): void {
  if (subscriptionStarted) return;
  subscriptionStarted = true;
  listen(EXTENSIONS_CHANGED_EVENT, () => {
    void useExtensionStore.getState().fetch();
  }).catch(() => {
    subscriptionStarted = false;
  });
}

ensureExtensionsChangedListener();
