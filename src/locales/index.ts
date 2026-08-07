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

export type { TranslationKey };

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

/** Locales shipped as beta: UI labels show (Beta); missing keys fall back to English. */
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

function resolveLocale(locale: string): SupportedLocale {
  if (locale in locales) return locale as SupportedLocale;
  return 'en';
}

export function getTranslation(
  locale: SupportedLocale | string,
  key: TranslationKey,
  params?: Record<string, string | number>,
): string {
  const resolved = resolveLocale(locale);
  const dict = locales[resolved];
  let text = dict[key] ?? en[key] ?? zhCN[key] ?? key;
  if (params) {
    for (const [k, v] of Object.entries(params)) {
      text = text.replace(new RegExp(`\\{${k}\\}`, 'g'), String(v));
    }
  }
  return text;
}

export function getAllTranslations(locale: SupportedLocale | string): Record<string, string> {
  return locales[resolveLocale(locale)];
}
