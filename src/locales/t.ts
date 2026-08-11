import { getTranslation, type I18nKey, type SupportedLocale } from './index';
import { useSettingsStore } from '../stores/settingsStore';

/**
 * Standalone translation function for use outside React components (stores, utils).
 * Reads the current language from the settings store synchronously.
 */
export function t(key: I18nKey, params?: Record<string, string | number>): string {
  const lang = (useSettingsStore.getState().settings.language ?? 'en') as SupportedLocale;
  return getTranslation(lang, key, params);
}
