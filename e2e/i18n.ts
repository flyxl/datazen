/**
 * E2E test i18n helper.
 *
 * Imports the zh-CN locale and provides a `t()` function so spec files
 * can reference UI text by key instead of hardcoding Chinese strings.
 */
import zhCN from '../src/locales/zh-CN.js';

type TranslationKey = keyof typeof zhCN;

/** Legacy E2E keys that were renamed in locale files. */
const KEY_ALIASES: Record<string, TranslationKey> = {
  'action.newConnection': 'common.newConnection',
  'action.backup': 'common.backupDatabase',
  'action.dataSync': 'common.dataSync',
  'backup.title': 'common.backupDatabase',
  'connWin.newQuery': 'common.newQuery',
  'connWin.copyDDL': 'common.copyDdl',
  'connWin.newTable': 'common.newTable',
  'sync.windowTitle': 'common.dataSyncTitle',
};

/**
 * Resolve a translation key to its zh-CN string value.
 * Supports simple `{param}` interpolation.
 */
export function t(key: TranslationKey | string, params?: Record<string, string | number>): string {
  const resolved = (KEY_ALIASES[key] ?? key) as TranslationKey;
  let value: string = zhCN[resolved];
  if (value === undefined) {
    throw new Error(`Missing zh-CN translation for key: ${String(key)}`);
  }
  if (params) {
    for (const [k, v] of Object.entries(params)) {
      value = value.replace(`{${k}}`, String(v));
    }
  }
  return value;
}
