/**
 * Full dictionaries (eager + lazy). Import only from tests or tooling —
 * this module pulls every domain into the same chunk.
 */
import type { TranslationKey } from './zh-CN';
import en from './en';
import zhCN from './zh-CN';
import type { BuiltinLocale } from './builtinLocales';

export const fullBuiltinLocales: Record<BuiltinLocale, Record<TranslationKey, string>> = {
  en: en as Record<TranslationKey, string>,
  'zh-CN': zhCN as Record<TranslationKey, string>,
};

export { en, zhCN };
