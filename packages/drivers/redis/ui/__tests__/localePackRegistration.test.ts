/** @vitest-environment node */
/**
 * Driver locale-pack registration guard (track i18n-drivers, BUG-001).
 *
 * The whole point of the redis `locales/` directory is that the pack reaches
 * the shared `@datazen/ui` registry *without host cooperation*, via the side
 * effect mounted on the driver UI entry module:
 *
 *   src/extensions/generated.ts → ui/shared/meta.ts → locales/index.ts → registerTranslations()
 *
 * This suite therefore imports `../shared/meta` and deliberately never
 * imports `locales/index.ts`: if that side-effect line is ever dropped or
 * "cleaned up" out of meta.ts, every assertion below turns red (the other 218
 * redis UI cases are language-agnostic by design and cannot see it).
 *
 * It also pins the two conventions the self-registration contract relies on:
 *   - the locale-code literals registered by the pack are exactly the language
 *     files shipped in `locales/` (a new `xx.ts` without an `index.ts` import,
 *     or a renamed/mis-hyphenated code, fails here);
 *   - every key of every shipped dictionary is present in the registry with
 *     the shipped value.
 */
import { describe, expect, it } from 'vitest';
import { readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { getRegisteredTranslations, t } from '@datazen/ui';

// Real loading path — the module generated.ts imports. Not locales/index.ts.
import '../shared/meta';

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

const PACK_PREFIX = 'redis.';

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

describe('[tester] redis locale pack self-registration via ui entry module', () => {
  it('registers the pack through the meta.ts side effect (t() resolves real strings)', () => {
    // The active locale is 'en' (only the host wires setLocale, and it does
    // not run in this suite), so this is the app's normal lookup path.
    expect(t('redis.batchDelete')).toBe('Delete selected');
    expect(t('redis.console')).toBe('Console');
    // An unregistered key would echo verbatim — guard against a false green.
    expect(t('redis.definitelyNotAKey')).toBe('redis.definitelyNotAKey');
    expect(driverKeysOf('en').length).toBeGreaterThan(100);
  });

  it('keeps the registered locale-code literals in sync with locales/*.ts', () => {
    const fileCodes = readdirSync(localesDir)
      .filter((name) => name.endsWith('.ts') && name !== 'index.ts')
      .map((name) => name.replace(/\.ts$/, ''))
      .sort();

    // Unknown / missing language files must be reflected in BOTH locales/index.ts
    // and the code literals above, otherwise the pack silently ships dead files.
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
    // 'pt-BR' / 'zh-CN' / 'zh-TW' are the host literals; a mis-cased code would
    // register an unreachable dictionary (host and drivers never normalize).
    expect(driverKeysOf('pt-BR').length).toBeGreaterThan(0);
    expect(driverKeysOf('zh-CN').length).toBeGreaterThan(0);
    expect(driverKeysOf('zh-TW').length).toBeGreaterThan(0);
    expect(getRegisteredTranslations('pt_BR')).toEqual({});
    expect(getRegisteredTranslations('zh_cn')).toEqual({});
  });
});
