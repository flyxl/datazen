import { create } from 'zustand';
import { getPlatformOrNull } from '../platform/contracts';
import {
  LANGUAGE_PACKS_CHANGED_EVENT,
  languagePackCommands,
  type LanguagePackSummary,
} from '../commands/languagePacks';
import { toErrorMessage } from '../lib/errors';

interface LanguagePackStore {
  packs: LanguagePackSummary[];
  loaded: boolean;
  error: string | null;
  fetch: () => Promise<void>;
  remove: (code: string) => Promise<void>;
  byCode: (code: string) => LanguagePackSummary | undefined;
}

export const useLanguagePackStore = create<LanguagePackStore>((set, get) => ({
  packs: [],
  loaded: false,
  error: null,

  fetch: async () => {
    try {
      const packs = await languagePackCommands.listLanguagePacks();
      set({ packs, loaded: true, error: null });
    } catch (e) {
      set({ loaded: true, error: toErrorMessage(e) });
    }
  },

  remove: async (code) => {
    try {
      await languagePackCommands.removeLanguagePack(code);
      await get().fetch();
    } catch (e) {
      set({ error: toErrorMessage(e) });
      throw e;
    }
  },

  byCode: (code) => get().packs.find((p) => p.code === code),
}));

let subscriptionStarted = false;

/**
 * Register the `language-packs:changed` refresh listener exactly once per
 * module instance (mirror of `ensureWappsChangedListener`): another window
 * installing/removing a pack refreshes this list here.
 */
export function ensureLanguagePacksChangedListener(): void {
  if (subscriptionStarted) return;
  subscriptionStarted = true;
  try {
    const events = getPlatformOrNull()?.events;
    if (!events) {
      // Platform not assembled yet (unit tests / pre-bootstrap): retry later.
      subscriptionStarted = false;
      return;
    }
    const p = events.listenCrossWindow(LANGUAGE_PACKS_CHANGED_EVENT, () => {
      void useLanguagePackStore.getState().fetch();
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
