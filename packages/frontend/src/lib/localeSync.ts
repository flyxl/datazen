import { setLocale } from '@datazen/ui';
import { useSettingsStore } from '../stores/settingsStore';
import { ensureLanguagePackMessages, isBuiltinLocale } from './languagePackRuntime';
// Side effect: registers the eager host dictionaries into the shared
// @datazen/ui translation registry (driver and extension packs register
// themselves through their own locales entry module).
import '../locales';

function currentLanguage(): string {
  return useSettingsStore.getState().settings?.language ?? 'en';
}

/**
 * Apply a language selection to the shared engine.
 *
 * Builtin locales switch **synchronously** — the historical contract every
 * consumer of a settingsStore.language mutation relies on (render-then-read
 * tests pin it). Pack-backed locales register their messages first and switch
 * in the microtask after (a slow pack read must never block the UI thread,
 * and it must never override a later switch — last writer wins).
 */
function applyLanguage(next: string): void {
  if (isBuiltinLocale(next)) {
    setLocale(next);
    return;
  }
  void ensureLanguagePackMessages(next).then(() => {
    // Both sides normalize the missing-language fallback (`?? 'en'`) so the
    // comparison is apples-to-apples.
    const current = useSettingsStore.getState().settings?.language ?? 'en';
    if (current === next) {
      setLocale(next);
    }
  });
}

/**
 * Host entry wiring (called once from main.tsx): seeds the shared i18n engine
 * with the persisted language and keeps it in sync with settingsStore changes.
 * This is the ONLY place where the host drives `setLocale`.
 *
 * External language packs: the persisted language may be provided by an
 * installed pack, so pack-backed locales are registered before switching
 * (async; one English flash at most — the same trade-off as async-applied
 * theme packs). Builtin locales stay synchronous.
 *
 * Returns the unsubscribe handle (used by tests / hot-reload cleanup).
 */
export function startLocaleSync(): () => void {
  applyLanguage(currentLanguage());
  return useSettingsStore.subscribe((state, prevState) => {
    const next = state.settings?.language ?? 'en';
    if (next === prevState.settings?.language) {
      return;
    }
    applyLanguage(next);
  });
}
