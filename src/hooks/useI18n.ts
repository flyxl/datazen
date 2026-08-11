import { useCallback } from 'react';
import { useSettingsStore } from '../stores/settingsStore';
import {
  getTranslation,
  type I18nKey,
  type SupportedLocale,
} from '../locales';

export function useI18n() {
  const language = useSettingsStore((s) => s.settings.language) as SupportedLocale;

  const t = useCallback(
    (key: I18nKey, params?: Record<string, string | number>) =>
      getTranslation(language ?? 'en', key, params),
    [language],
  );

  return { t, language };
}
