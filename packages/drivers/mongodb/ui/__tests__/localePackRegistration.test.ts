/** @vitest-environment node */
/**
 * Driver locale-pack registration guard (track i18n-drivers, BUG-001).
 *
 * The mongodb pack must reach the shared `@datazen/ui` registry without host
 * cooperation, through the side effect mounted on the driver UI entry module:
 *
 *   src/extensions/generated.ts → ui/meta.ts → locales/index.ts → registerTranslations()
 *
 * This suite imports `../meta` and deliberately never imports
 * `locales/index.ts`, so removing that side-effect line from the entry module
 * turns the assertions below red (`ui/mongodbFind.test.ts` is logic-only and
 * cannot see the registry at all).
 *
 * It also pins the conventions the contract relies on: the registered locale
 * codes are exactly the language files shipped in `locales/`, and every key of
 * every shipped dictionary lands in the registry with its shipped value.
 */
import { describe, expect, it } from 'vitest';
import { readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { getRegisteredTranslations, t } from '@datazen/ui';

// Real loading path — the module generated.ts imports. Not locales/index.ts.
import '../meta';

import de from '../../locales/de';
import en from '../../locales/en';
import es from '../../locales/es';
import fr from '../../locales/fr';
import ja from '../../locales/ja';
import ko from '../../locales/ko';
import ptBR from '../../locales/pt-BR';
import ru from '../../locales/ru';
import zhCN from '../../locales/zh-CN';
import zhTW from '../../locales/zh-TW';

const PACK_PREFIX = 'mongo.';

/** Locale codes the pack registers — must match host `src/locales/` literals. */
const SHIPPED_LOCALES: Record<string, Record<string, string>> = {
  en,
  'zh-CN': zhCN,
  'zh-TW': zhTW,
  de,
  es,
  fr,
  ja,
  ko,
  'pt-BR': ptBR,
  ru,
};

const localesDir = resolve(import.meta.dirname, '../../locales');

const driverKeysOf = (locale: string): string[] =>
  Object.keys(getRegisteredTranslations(locale)).filter((key) => key.startsWith(PACK_PREFIX));

describe('[tester] mongodb locale pack self-registration via ui entry module', () => {
  it('registers the pack through the meta.ts side effect (t() resolves real strings)', () => {
    // Active locale is 'en': only the host wires setLocale, and it does not
    // run in this suite.
    expect(t('mongo.collections')).toBe('Collections');
    expect(t('mongo.noIdHint')).toBe('Document must include an _id field to save or delete');
    // An unregistered key would echo verbatim — guard against a false green.
    expect(t('mongo.definitelyNotAKey')).toBe('mongo.definitelyNotAKey');
    expect(driverKeysOf('en')).toEqual(Object.keys(en));
  });

  it('keeps the registered locale-code literals in sync with locales/*.ts', () => {
    const fileCodes = readdirSync(localesDir)
      .filter((name) => name.endsWith('.ts') && name !== 'index.ts')
      .map((name) => name.replace(/\.ts$/, ''))
      .sort();

    expect(fileCodes).toEqual([...Object.keys(SHIPPED_LOCALES)].sort());
  });

  it('registers every key of every shipped dictionary with its shipped value', () => {
    for (const [locale, dict] of Object.entries(SHIPPED_LOCALES)) {
      const registered = getRegisteredTranslations(locale);
      const notRegistered = Object.keys(dict).filter((key) => !(key in registered));
      expect(notRegistered, `locales/${locale}.ts keys missing from the registry`).toEqual([]);

      const diverged = Object.entries(dict)
        .filter(([key, value]) => registered[key] !== value)
        .map(([key]) => key);
      expect(diverged, `locales/${locale}.ts values not registered verbatim`).toEqual([]);
    }
  });

  it('registers each locale under its exact hyphenated code only', () => {
    expect(driverKeysOf('pt-BR')).toEqual(Object.keys(ptBR));
    expect(driverKeysOf('zh-CN')).toEqual(Object.keys(zhCN));
    expect(driverKeysOf('zh-TW')).toEqual(Object.keys(zhTW));
    expect(getRegisteredTranslations('pt_BR')).toEqual({});
    expect(getRegisteredTranslations('zh_cn')).toEqual({});
  });
});
