import zhCN, { type TranslationKey } from './zh-CN';
import en from './en';

export type { TranslationKey };
export type SupportedLocale = 'zh-CN' | 'en';

const locales: Record<SupportedLocale, Record<TranslationKey, string>> = {
  'zh-CN': zhCN,
  en,
};

export function getTranslation(locale: SupportedLocale, key: TranslationKey, params?: Record<string, string | number>): string {
  const dict = locales[locale] ?? locales['zh-CN'];
  let text = dict[key] ?? zhCN[key] ?? key;
  if (params) {
    for (const [k, v] of Object.entries(params)) {
      text = text.replace(new RegExp(`\\{${k}\\}`, 'g'), String(v));
    }
  }
  return text;
}

export function getAllTranslations(locale: SupportedLocale): Record<string, string> {
  return locales[locale] ?? locales['zh-CN'];
}
