import { create } from 'zustand';
import { listen } from '@tauri-apps/api/event';
import { WAPPS_CHANGED_EVENT, wappCommands } from '../commands/wapps';
import { toErrorMessage } from '../lib/errors';
import type { WappSummary } from '../types/wapp';

interface WappStore {
  wapps: WappSummary[];
  loaded: boolean;
  error: string | null;
  fetch: () => Promise<void>;
  /** Optimistically flips the flag, then reconciles with an authoritative refetch. */
  setEnabled: (id: string, enabled: boolean) => Promise<void>;
  remove: (id: string) => Promise<void>;
  byId: (id: string) => WappSummary | undefined;
}

export const useWappStore = create<WappStore>((set, get) => ({
  wapps: [],
  loaded: false,
  error: null,

  fetch: async () => {
    try {
      const wapps = await wappCommands.listWapps();
      set({ wapps, loaded: true, error: null });
    } catch (e) {
      set({ loaded: true, error: toErrorMessage(e) });
    }
  },

  setEnabled: async (id, enabled) => {
    const previous = get().wapps;
    const updated = previous.map((p) => (p.id === id ? { ...p, enabled } : p));
    set({
      wapps: updated,
      error: null,
    });
    try {
      await wappCommands.setWappEnabled(id, enabled);
      await get().fetch();
    } catch (e) {
      await get().fetch();
      set({ error: toErrorMessage(e) });
      throw e;
    }
  },

  remove: async (id) => {
    try {
      await wappCommands.removeWapp(id);
      await get().fetch();
    } catch (e) {
      set({ error: toErrorMessage(e) });
      throw e;
    }
  },

  byId: (id) => get().wapps.find((p) => p.id === id),
}));

let subscriptionStarted = false;

/**
 * Register the `wapps:changed` refresh listener exactly once per module
 * instance. The failure path resets the guard so a later retry can succeed
 * (e.g. when first attempted outside the Tauri runtime).
 */
export function ensureWappsChangedListener(): void {
  if (subscriptionStarted) return;
  subscriptionStarted = true;
  try {
    const p = listen(WAPPS_CHANGED_EVENT, () => {
      void useWappStore.getState().fetch();
    });
    if (p && typeof p.catch === 'function') {
      p.catch(() => {
        subscriptionStarted = false;
      });
    }
  } catch {
    subscriptionStarted = false;
  }
}

ensureWappsChangedListener();
