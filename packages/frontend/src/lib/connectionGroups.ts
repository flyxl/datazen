import type { TranslationKey } from '../locales';

/** Stable storage keys for built-in connection groups (locale-independent). */
export const PRESET_GROUPS = {
  production: 'preset:production',
  development: 'preset:development',
  testing: 'preset:testing',
} as const;

export const PRESET_GROUP_OPTIONS: { key: string; i18nKey: TranslationKey }[] = [
  { key: PRESET_GROUPS.production, i18nKey: 'newConn.groupProd' },
  { key: PRESET_GROUPS.development, i18nKey: 'newConn.groupDev' },
  { key: PRESET_GROUPS.testing, i18nKey: 'newConn.groupTest' },
];

/** Localized label for a group key; custom keys are returned as-is. */
export function formatGroupLabel(
  key: string,
  t: (k: TranslationKey, params?: Record<string, string | number>) => string,
): string {
  const preset = PRESET_GROUP_OPTIONS.find((o) => o.key === key);
  if (preset) return t(preset.i18nKey);
  return key;
}
