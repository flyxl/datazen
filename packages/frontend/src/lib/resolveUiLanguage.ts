/** Supported UI languages (must match Rust `i18n_locale::BUILTIN`). */
export const SUPPORTED_UI_LANGUAGES = ['en', 'zh-CN'] as const;

export type SupportedUiLanguage = (typeof SUPPORTED_UI_LANGUAGES)[number];

function normalizeLocaleTag(locale: string): string {
  return locale.trim().replace(/_/g, '-');
}

function resolveBuiltinUiLanguage(normalized: string): SupportedUiLanguage | null {
  if (!normalized) return null;
  for (const supported of SUPPORTED_UI_LANGUAGES) {
    if (normalized.toLowerCase() === supported.toLowerCase()) {
      return supported;
    }
  }
  const lower = normalized.toLowerCase();
  if (lower.startsWith('zh')) return 'zh-CN';
  if (lower.startsWith('en')) return 'en';
  return null;
}

/**
 * Map an OS locale tag to a supported UI language code.
 * Mirrors Rust `resolve_ui_language`.
 */
export function resolveUiLanguage(systemLocale: string): SupportedUiLanguage {
  return resolveBuiltinUiLanguage(normalizeLocaleTag(systemLocale)) ?? 'en';
}

/**
 * Normalize a persisted/user-selected UI language setting.
 *
 * Built-in locales are canonicalized (e.g. en-US -> en, zh -> zh-CN), while
 * non-builtin codes (e.g. installed language packs like "ja") are preserved so
 * the app can load and apply them. Empty values fall back to English.
 */
export function normalizeSettingsLanguage(language: string): string {
  const normalized = normalizeLocaleTag(language);
  if (!normalized) return 'en';
  return resolveBuiltinUiLanguage(normalized) ?? normalized;
}
