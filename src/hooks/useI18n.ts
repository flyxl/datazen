import { useCallback } from 'react';
import { useSettingsStore } from '../stores/settingsStore';
import { getTranslation, type SupportedLocale, type TranslationKey } from '../locales';

export function useI18n() {
  const language = useSettingsStore((s) => s.settings.language) as SupportedLocale;

  const t = useCallback(
    (key: TranslationKey, params?: Record<string, string | number>) =>
      getTranslation(language ?? 'en', key, params),
    [language],
  );

  return { t, language };
}
