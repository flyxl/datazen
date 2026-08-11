import zhCN, { type TranslationKey } from './zh-CN';
import en from './en';
import zhTW from './zh-TW';
import es from './es';
import fr from './fr';
import de from './de';
import ja from './ja';
import ptBR from './pt-BR';
import ru from './ru';
import ko from './ko';
import type { MongoTranslationKey } from '../../packages/drivers/mongodb/locales/en';
import {
  PLUGIN_LOCALES,
  type PluginTranslationKey,
} from '../plugins/generated-locales';

export type { TranslationKey, PluginTranslationKey, MongoTranslationKey };

/** Host keys plus merged plugin keys from enabled drivers. */
export type I18nKey = TranslationKey | PluginTranslationKey | MongoTranslationKey;

export const SUPPORTED_LOCALES = [
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

export type SupportedLocale = (typeof SUPPORTED_LOCALES)[number];

/** Locales with complete translations (not English placeholders). */
export const FULLY_TRANSLATED_LOCALES = [...SUPPORTED_LOCALES] as const satisfies readonly SupportedLocale[];

/** Former beta locales — now empty; all supported locales are fully translated. */
export const BETA_LOCALES = [] as const satisfies readonly SupportedLocale[];

export function isBetaLocale(locale: string): boolean {
  return (BETA_LOCALES as readonly string[]).includes(locale);
}

const locales: Record<SupportedLocale, Record<TranslationKey, string>> = {
  en,
  'zh-CN': zhCN,
  'zh-TW': zhTW,
  es,
  fr,
  de,
  ja,
  'pt-BR': ptBR,
  ru,
  ko,
};

const pluginLocalesEn = PLUGIN_LOCALES.en;

function resolveLocale(locale: string): SupportedLocale {
  if (locale in locales) return locale as SupportedLocale;
  return 'en';
}

export function getTranslation(
  locale: SupportedLocale | string,
  key: I18nKey,
  params?: Record<string, string | number>,
): string {
  const resolved = resolveLocale(locale);
  const dict = locales[resolved];
  const pluginDict = PLUGIN_LOCALES[resolved];
  let text =
    dict[key as TranslationKey]
    ?? pluginDict[key]
    ?? en[key as TranslationKey]
    ?? pluginLocalesEn[key]
    ?? zhCN[key as TranslationKey]
    ?? key;
  if (params) {
    for (const [k, v] of Object.entries(params)) {
      text = text.replace(new RegExp(`\\{${k}\\}`, 'g'), String(v));
    }
  }
  return text;
}

/** Host locale strings only (excludes plugin driver keys). */
export function getHostTranslations(locale: SupportedLocale | string): Record<string, string> {
  return locales[resolveLocale(locale)];
}

/** Host + merged plugin locale strings for the active driver set. */
export function getAllTranslations(locale: SupportedLocale | string): Record<string, string> {
  const resolved = resolveLocale(locale);
  return { ...locales[resolved], ...PLUGIN_LOCALES[resolved] };
}
