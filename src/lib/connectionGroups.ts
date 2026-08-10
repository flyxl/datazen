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

/**
 * Legacy display literals (Chinese + English + other locale defaultGroup / group* labels)
 * → stable preset keys. Custom group names are not listed and pass through unchanged.
 */
const LEGACY_GROUP_ALIASES: Record<string, string> = {
  // Simplified Chinese
  生产环境: PRESET_GROUPS.production,
  开发环境: PRESET_GROUPS.development,
  测试环境: PRESET_GROUPS.testing,
  // Traditional Chinese
  生產環境: PRESET_GROUPS.production,
  開發環境: PRESET_GROUPS.development,
  測試環境: PRESET_GROUPS.testing,
  // English (and fr groupProd)
  Production: PRESET_GROUPS.production,
  Development: PRESET_GROUPS.development,
  Testing: PRESET_GROUPS.testing,
  // de
  Produktion: PRESET_GROUPS.production,
  Entwicklung: PRESET_GROUPS.development,
  Testen: PRESET_GROUPS.testing,
  // es
  Producción: PRESET_GROUPS.production,
  Desarrollo: PRESET_GROUPS.development,
  Pruebas: PRESET_GROUPS.testing,
  // fr
  Développement: PRESET_GROUPS.development,
  Essai: PRESET_GROUPS.testing,
  // ja
  生産: PRESET_GROUPS.production,
  発達: PRESET_GROUPS.development,
  テスト: PRESET_GROUPS.testing,
  // ko
  생산: PRESET_GROUPS.production,
  개발: PRESET_GROUPS.development,
  테스트: PRESET_GROUPS.testing,
  // pt-BR
  Produção: PRESET_GROUPS.production,
  Desenvolvimento: PRESET_GROUPS.development,
  Teste: PRESET_GROUPS.testing,
  // ru
  Производство: PRESET_GROUPS.production,
  Разработка: PRESET_GROUPS.development,
  Тестирование: PRESET_GROUPS.testing,
};

/** Normalize a stored/typed group to a stable key; empty → undefined; custom names pass through. */
export function normalizeGroupKey(raw: string | undefined | null): string | undefined {
  if (raw == null) return undefined;
  const trimmed = raw.trim();
  if (!trimmed) return undefined;
  return LEGACY_GROUP_ALIASES[trimmed] ?? trimmed;
}

/** Localized label for a group key; custom keys are returned as-is. */
export function formatGroupLabel(
  key: string,
  t: (k: TranslationKey, params?: Record<string, string | number>) => string,
): string {
  const preset = PRESET_GROUP_OPTIONS.find((o) => o.key === key);
  if (preset) return t(preset.i18nKey);
  return key;
}

/** Dedupe and normalize a groups list while preserving first-seen order. */
export function normalizeGroupList(groups: string[]): string[] {
  const result: string[] = [];
  const seen = new Set<string>();
  for (const g of groups) {
    const key = normalizeGroupKey(g);
    if (!key || seen.has(key)) continue;
    seen.add(key);
    result.push(key);
  }
  return result;
}
