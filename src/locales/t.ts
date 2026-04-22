import { getTranslation, type SupportedLocale, type TranslationKey } from './index';
import { useSettingsStore } from '../stores/settingsStore';

/**
 * Standalone translation function for use outside React components (stores, utils).
 * Reads the current language from the settings store synchronously.
 */
export function t(key: TranslationKey, params?: Record<string, string | number>): string {
  const lang = (useSettingsStore.getState().settings.language ?? 'zh-CN') as SupportedLocale;
  return getTranslation(lang, key, params);
}
