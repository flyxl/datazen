import { useCallback } from 'react';
import { useSettingsStore } from '../stores/settingsStore';
import {
  getTranslation,
  type I18nKey,
  type SupportedLocale,
} from '../locales';

export function useI18n() {
  const language = useSettingsStore((s) => s.settings.language) as SupportedLocale;

  // Keep the hook usable by generic UI helpers whose translation callback is
  // intentionally `(key: string) => string`; the actual locale lookup remains
  // type-safe at the getTranslation boundary.
  const t = useCallback(
    (key: string, params?: Record<string, string | number>) =>
      getTranslation(language ?? 'en', key as I18nKey, params),
    [language],
  );

  return { t, language };
}
