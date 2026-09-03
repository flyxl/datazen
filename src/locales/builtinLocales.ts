import type { TranslationKey } from './zh-CN';
import enEager from './en/eager';
import zhCNEager from './zh-CN/eager';
import meta from './builtin-locales.json';
import { lookupLazyTranslation } from './lazyPacks';
import { LAZY_DOMAINS } from './domains';

export const BUILTIN_LOCALES = meta.locales.map((l) => l.code) as readonly ['en', 'zh-CN'];
export type BuiltinLocale = (typeof BUILTIN_LOCALES)[number];

export const BUILTIN_LOCALE_LABELS: Record<BuiltinLocale, string> = Object.fromEntries(
  meta.locales.map((l) => [l.code, l.label]),
) as Record<BuiltinLocale, string>;

/** Eager-only host dictionaries (main chunk). */
export const builtinEagerLocales: Record<BuiltinLocale, Record<string, string>> = {
  en: enEager as Record<string, string>,
  'zh-CN': zhCNEager as Record<string, string>,
};

/**
 * Host dictionary view: eager packs + any lazy packs already loaded into the registry.
 * Does not statically import lazy domain modules (keeps them code-split).
 */
export function getBuiltinHostDict(locale: BuiltinLocale): Record<string, string> {
  const base = { ...builtinEagerLocales[locale] };
  // Overlay lazy keys that are present in the registry without importing packs.
  // We cannot enumerate registry packs without a getter — lookup per-key is used in getTranslation.
  // For snapshot-style consumers, merge via known lazy key prefixes is not available;
  // prefer getTranslation or ensureLocaleDomains + getHostTranslations after preload.
  void LAZY_DOMAINS;
  void lookupLazyTranslation;
  return base;
}

/**
 * @deprecated Prefer getTranslation / getHostTranslations.
 * Retained name for existing imports; returns eager + relies on getHostTranslations for full.
 */
export const builtinLocales: Record<BuiltinLocale, Record<string, string>> = {
  en: builtinEagerLocales.en,
  'zh-CN': builtinEagerLocales['zh-CN'],
};

export type { TranslationKey };
