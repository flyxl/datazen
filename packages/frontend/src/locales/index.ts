import type { TranslationKey } from './zh-CN';
import {
  BUILTIN_LOCALES,
  builtinLocales,
  BUILTIN_LOCALE_LABELS,
  builtinEagerLocales,
  type BuiltinLocale,
} from './builtinLocales';
import {
  getLocale,
  getRegisteredTranslations,
  registerTranslations,
  setLocale,
  t as translate,
} from '@datazen/ui';

export type { TranslationKey };
export { BUILTIN_LOCALES, builtinLocales, BUILTIN_LOCALE_LABELS };
export type { BuiltinLocale };
export { ensureLocaleDomains, ensureAllLazyDomains, isDomainLoaded } from './lazyPacks';
export type { LazyDomain, LocaleDomain } from './domains';
export { LAZY_DOMAINS, EAGER_DOMAINS } from './domains';

/** Host keys; driver/extension keys are plain strings (no central key union). */
export type I18nKey = TranslationKey | (string & {});

// ── Single lookup engine ─────────────────────────────────────────────────────
// All runtime lookup / fallback / interpolation lives in @datazen/ui (`t`).
// This module owns no dictionary of its own: host dictionaries are pushed
// into the shared @datazen/ui registry on load (lazy domain packs register on
// demand via lazyPacks). Driver and extension packs register *themselves*
// through their own `locales/index.ts` side-effect module — the host never
// aggregates or imports them. localeSync wires settingsStore.language →
// setLocale.
for (const locale of BUILTIN_LOCALES) {
  registerTranslations({ [locale]: builtinEagerLocales[locale] });
}

const extensionLocales = new Map<string, string>();

export function getAvailableLocales(): string[] {
  return [...BUILTIN_LOCALES, ...extensionLocales.keys()];
}

export type SupportedLocale = string;

export function registerLocale(
  locale: string,
  label: string,
  translations: Record<string, string>,
): void {
  extensionLocales.set(locale, label);
  registerTranslations({ [locale]: translations });
}

export function unregisterLocale(locale: string): void {
  extensionLocales.delete(locale);
}

export function getExtensionLocales(): Array<{ value: string; label: string }> {
  return [...extensionLocales.entries()].map(([value, label]) => ({
    value,
    label,
  }));
}

/**
 * Locale-parameterized lookup used by tooling and tests. Delegates to the
 * single @datazen/ui engine: for the active locale this is a direct `t`
 * call; for a foreign locale the active locale is swapped for the duration
 * of the synchronous lookup (never in React render paths).
 */
export function getTranslation(
  locale: SupportedLocale | string,
  key: I18nKey,
  params?: Record<string, string | number>,
): string {
  const previous = getLocale();
  if (previous === locale) {
    return translate(key, params);
  }
  setLocale(locale);
  try {
    return translate(key, params);
  } finally {
    setLocale(previous);
  }
}

/**
 * Host eager locale strings only — no lazy domain packs, no driver or
 * extension registrations. Use {@link getAllTranslations} for a snapshot of
 * everything currently present in the shared registry.
 * For a complete host-only snapshot of all keys, call ensureAllLazyDomains(locale)
 * first or import from './fullLocales'.
 */
export function getHostTranslations(locale: SupportedLocale | string): Record<string, string> {
  if (isBuiltinLocale(locale)) {
    // Start from eager; overlay lazy via key re-resolution is not enumerable.
    // Consumers that need every key should use fullLocales in tests.
    return { ...builtinEagerLocales[locale] };
  }
  return { ...builtinEagerLocales.en };
}

/**
 * Host + driver + extension locale strings currently present in the shared
 * @datazen/ui registry. Non-built-in locale codes fall back to `en` (same
 * contract as before the i18n runtime was centralised).
 */
export function getAllTranslations(locale: SupportedLocale | string): Record<string, string> {
  return getRegisteredTranslations(isBuiltinLocale(locale) ? locale : 'en');
}

function isBuiltinLocale(locale: string): locale is BuiltinLocale {
  return (BUILTIN_LOCALES as readonly string[]).includes(locale);
}
