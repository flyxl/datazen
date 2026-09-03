import type { TranslationKey } from './zh-CN';
import {
  BUILTIN_LOCALES,
  builtinLocales,
  builtinEagerLocales,
  BUILTIN_LOCALE_LABELS,
  type BuiltinLocale,
} from './builtinLocales';
import type { MongoTranslationKey } from '../../packages/drivers/mongodb/locales/en';
import { PLUGIN_LOCALES, type PluginTranslationKey } from '../plugins/generated-locales';
import { lookupLazyTranslation } from './lazyPacks';

export type { TranslationKey, PluginTranslationKey, MongoTranslationKey };
export { BUILTIN_LOCALES, builtinLocales, BUILTIN_LOCALE_LABELS };
export type { BuiltinLocale };
export { ensureLocaleDomains, ensureAllLazyDomains, isDomainLoaded } from './lazyPacks';
export type { LazyDomain, LocaleDomain } from './domains';
export { LAZY_DOMAINS, EAGER_DOMAINS } from './domains';

/** Host keys plus merged plugin keys from enabled drivers. */
export type I18nKey = TranslationKey | PluginTranslationKey | MongoTranslationKey;

const extensionLocales = new Map<string, { label: string; translations: Record<string, string> }>();

export function getAvailableLocales(): string[] {
  return [...BUILTIN_LOCALES, ...extensionLocales.keys()];
}

export type SupportedLocale = string;

export function registerLocale(
  locale: string,
  label: string,
  translations: Record<string, string>,
): void {
  extensionLocales.set(locale, { label, translations });
}

export function unregisterLocale(locale: string): void {
  extensionLocales.delete(locale);
}

export function getExtensionLocales(): Array<{ value: string; label: string }> {
  return [...extensionLocales.entries()].map(([value, { label }]) => ({
    value,
    label,
  }));
}

const pluginLocalesEn = PLUGIN_LOCALES.en;

function isBuiltinLocale(locale: string): locale is BuiltinLocale {
  return (BUILTIN_LOCALES as readonly string[]).includes(locale);
}

function hostLookup(locale: BuiltinLocale, key: string): string | undefined {
  return (
    builtinEagerLocales[locale][key] ??
    lookupLazyTranslation(locale, key)
  );
}

export function getTranslation(
  locale: SupportedLocale | string,
  key: I18nKey,
  params?: Record<string, string | number>,
): string {
  let text: string | undefined;

  if (isBuiltinLocale(locale)) {
    const pluginDict = PLUGIN_LOCALES[locale] as Record<string, string> | undefined;
    text =
      hostLookup(locale, key) ??
      pluginDict?.[key] ??
      hostLookup('en', key) ??
      pluginLocalesEn[key] ??
      hostLookup('zh-CN', key);
  } else {
    const ext = extensionLocales.get(locale);
    text =
      ext?.translations[key] ??
      hostLookup('en', key) ??
      pluginLocalesEn[key] ??
      hostLookup('zh-CN', key);
  }

  text = text ?? key;

  if (params) {
    for (const [k, v] of Object.entries(params)) {
      text = text.replace(new RegExp(`\\{${k}\\}`, 'g'), String(v));
    }
  }
  return text;
}

/**
 * Host locale strings only (excludes plugin driver keys).
 * Includes eager packs plus any lazy packs already loaded for this locale.
 * For a complete snapshot of all keys, call ensureAllLazyDomains(locale) first
 * or import from './fullLocales'.
 */
export function getHostTranslations(locale: SupportedLocale | string): Record<string, string> {
  if (isBuiltinLocale(locale)) {
    // Start from eager; overlay lazy via key re-resolution is not enumerable.
    // Consumers that need every key should use fullLocales in tests.
    return { ...builtinEagerLocales[locale] };
  }
  const ext = extensionLocales.get(locale);
  return ext?.translations ?? { ...builtinEagerLocales.en };
}

/** Host + merged plugin locale strings for the active driver set. */
export function getAllTranslations(locale: SupportedLocale | string): Record<string, string> {
  if (isBuiltinLocale(locale)) {
    return { ...getHostTranslations(locale), ...PLUGIN_LOCALES[locale] };
  }
  const ext = extensionLocales.get(locale);
  return { ...(ext?.translations ?? builtinEagerLocales.en), ...PLUGIN_LOCALES.en };
}
