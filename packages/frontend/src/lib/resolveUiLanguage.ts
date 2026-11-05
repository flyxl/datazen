/** Supported UI languages (must match Rust `i18n_locale::BUILTIN`). */
export const SUPPORTED_UI_LANGUAGES = ['en', 'zh-CN'] as const;

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

  if (lower.startsWith('zh')) return 'zh-CN';
  if (lower.startsWith('en')) return 'en';

  return 'en';
}
