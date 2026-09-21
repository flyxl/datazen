import { setLocale } from '@datazen/ui';
import { useSettingsStore } from '../stores/settingsStore';
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
 * Returns the unsubscribe handle (used by tests / hot-reload cleanup).
 */
export function startLocaleSync(): () => void {
  setLocale(currentLanguage());
  return useSettingsStore.subscribe((state, prevState) => {
    const next = state.settings?.language ?? 'en';
    if (next !== prevState.settings?.language) {
      setLocale(next);
    }
  });
}
