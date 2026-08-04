/**
 * E2E test i18n helper.
 *
 * Imports the zh-CN locale and provides a `t()` function so spec files
 * can reference UI text by key instead of hardcoding Chinese strings.
 */
import zhCN from '../src/locales/zh-CN.js';

type TranslationKey = keyof typeof zhCN;

/**
 * Resolve a translation key to its zh-CN string value.
 * Supports simple `{param}` interpolation.
 */
export function t(key: TranslationKey, params?: Record<string, string | number>): string {
  let value: string = zhCN[key];
  if (params) {
    for (const [k, v] of Object.entries(params)) {
      value = value.replace(`{${k}}`, String(v));
    }
  }
  return value;
}
