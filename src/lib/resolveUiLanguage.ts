/** Supported UI language codes (must match Rust `i18n_locale::SUPPORTED`). */
export const SUPPORTED_UI_LANGUAGES = [
  'en',
  'zh-CN',
  'zh-TW',
  'es',
  'fr',
  'de',
  'ja',
  'pt-BR',
  'ru',
  'ko',
] as const;

export type SupportedUiLanguage = (typeof SUPPORTED_UI_LANGUAGES)[number];

/**
 * Map an OS locale tag to a supported UI language code.
 * Mirrors Rust `resolve_ui_language`.
 */
export function resolveUiLanguage(systemLocale: string): SupportedUiLanguage {
  const normalized = systemLocale.trim().replace(/_/g, '-');
  if (!normalized) {
    return 'en';
  }

  for (const supported of SUPPORTED_UI_LANGUAGES) {
    if (normalized.toLowerCase() === supported.toLowerCase()) {
      return supported;
    }
  }

  const lower = normalized.toLowerCase();

  if (lower.startsWith('zh-hans') || lower.startsWith('zh-cn')) {
    return 'zh-CN';
  }
  if (
    lower.startsWith('zh-hant') ||
    lower.startsWith('zh-tw') ||
    lower.startsWith('zh-hk') ||
    lower.startsWith('zh-mo')
  ) {
    return 'zh-TW';
  }
  if (lower.startsWith('zh')) {
    return 'zh-CN';
  }

  if (lower === 'pt' || lower.startsWith('pt-')) {
    return 'pt-BR';
  }

  if (lower.startsWith('en')) return 'en';
  if (lower.startsWith('es')) return 'es';
  if (lower.startsWith('fr')) return 'fr';
  if (lower.startsWith('de')) return 'de';
  if (lower.startsWith('ja')) return 'ja';
  if (lower.startsWith('ko')) return 'ko';
  if (lower.startsWith('ru')) return 'ru';

  return 'en';
}
