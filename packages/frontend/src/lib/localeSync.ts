import { setLocale } from '@datazen/ui';
import { useSettingsStore } from '../stores/settingsStore';
import { ensureLanguagePackMessages } from './languagePackRuntime';
// Side effect: registers the eager host dictionaries into the shared
// @datazen/ui translation registry (driver and extension packs register
// themselves through their own locales entry module).
import '../locales';

function currentLanguage(): string {
  return useSettingsStore.getState().settings?.language ?? 'en';
}

/**
 * Host entry wiring (called once from main.tsx): seeds the shared i18n engine
 * with the persisted language and keeps it in sync with settingsStore changes.
 * This is the ONLY place where the host drives `setLocale`.
 *
 * External language packs: the persisted language may be provided by an
 * installed pack, so the target locale's pack messages are ensured *before*
 * switching (async). Fast language changes are guarded last-writer-wins — a
 * slow pack read for `ja` must never override a later switch to `en`.
 *
 * Returns the unsubscribe handle (used by tests / hot-reload cleanup).
 */
export function startLocaleSync(): () => void {
  // First frame: builtin locales switch synchronously; pack-backed locales
  // register their messages first, then switch (one English flash at most,
  // same trade-off as async-applied theme packs).
  void ensureLanguagePackMessages(currentLanguage()).then(() => {
    setLocale(currentLanguage());
  });
  return useSettingsStore.subscribe((state, prevState) => {
    const next = state.settings?.language ?? 'en';
    if (next === prevState.settings?.language) {
      return;
    }
    void ensureLanguagePackMessages(next).then(() => {
      // Last-writer-wins: a slow pack read for an earlier choice must never
      // override a later switch. Both sides normalize the missing-language
      // fallback (`?? 'en'`) so the comparison is apples-to-apples.
      const current = useSettingsStore.getState().settings?.language ?? 'en';
      if (current === next) {
        setLocale(next);
      }
    });
  });
}
