import type { TranslationKey } from './zh-CN';
import en from './en';
import zhCN from './zh-CN';
import {
  BUILTIN_LOCALES,
  builtinLocales,
  BUILTIN_LOCALE_LABELS,
  type BuiltinLocale,
} from './builtinLocales';
import type { MongoTranslationKey } from '../../packages/drivers/mongodb/locales/en';
import { PLUGIN_LOCALES, type PluginTranslationKey } from '../plugins/generated-locales';

export type { TranslationKey, PluginTranslationKey, MongoTranslationKey };
export { BUILTIN_LOCALES, builtinLocales, BUILTIN_LOCALE_LABELS };
export type { BuiltinLocale };

/** Host keys plus merged plugin keys from enabled drivers. */
export type I18nKey = TranslationKey | PluginTranslationKey | MongoTranslationKey;

/**
 * Registered extension locale packs (added at runtime by language plugins).
 * Key = locale code, Value = { label, translations }.
 */
const extensionLocales = new Map<string, { label: string; translations: Record<string, string> }>();

/** All available locale codes (built-in + extensions). */
export function getAvailableLocales(): string[] {
  return [...BUILTIN_LOCALES, ...extensionLocales.keys()];
}

export type SupportedLocale = string;

/**
 * Register a language extension pack at runtime.
 * Plugins call this to add support for additional locales.
 */
export function registerLocale(
  locale: string,
  label: string,
  translations: Record<string, string>,
): void {
  extensionLocales.set(locale, { label, translations });
}

/** Unregister a previously registered extension locale. */
export function unregisterLocale(locale: string): void {
  extensionLocales.delete(locale);
}

/** Get all registered extension locales with their display labels. */
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

export function getTranslation(
  locale: SupportedLocale | string,
  key: I18nKey,
  params?: Record<string, string | number>,
): string {
  let text: string | undefined;

  if (isBuiltinLocale(locale)) {
    const dict = builtinLocales[locale];
    const pluginDict = PLUGIN_LOCALES[locale] as Record<string, string> | undefined;
    text =
      dict[key as TranslationKey] ??
      pluginDict?.[key] ??
      en[key as TranslationKey] ??
      pluginLocalesEn[key] ??
      zhCN[key as TranslationKey];
  } else {
    const ext = extensionLocales.get(locale);
    text =
      ext?.translations[key] ??
      en[key as TranslationKey] ??
      pluginLocalesEn[key] ??
      zhCN[key as TranslationKey];
  }

  text = text ?? key;

  if (params) {
    for (const [k, v] of Object.entries(params)) {
      text = text.replace(new RegExp(`\\{${k}\\}`, 'g'), String(v));
    }
  }
  return text;
}

/** Host locale strings only (excludes plugin driver keys). */
export function getHostTranslations(locale: SupportedLocale | string): Record<string, string> {
  if (isBuiltinLocale(locale)) return builtinLocales[locale];
  const ext = extensionLocales.get(locale);
  return ext?.translations ?? builtinLocales.en;
}

/** Host + merged plugin locale strings for the active driver set. */
export function getAllTranslations(locale: SupportedLocale | string): Record<string, string> {
  if (isBuiltinLocale(locale)) {
    return { ...builtinLocales[locale], ...PLUGIN_LOCALES[locale] };
  }
  const ext = extensionLocales.get(locale);
  return { ...(ext?.translations ?? builtinLocales.en), ...PLUGIN_LOCALES.en };
}
