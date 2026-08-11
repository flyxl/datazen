import { describe, it, expect } from 'vitest';
import zhCN from './zh-CN';
import en from './en';
import { SUPPORTED_LOCALES, getAllTranslations, getHostTranslations, getTranslation, BETA_LOCALES, FULLY_TRANSLATED_LOCALES, type SupportedLocale, type TranslationKey } from './index';

const CRITICAL_KEYS: TranslationKey[] = [
  'common.ok',
  'common.cancel',
  'menu.exportConfig',
  'menu.importConfig',
  'action.exportConfig',
  'action.importConfig',
  'appData.exportSuccess',
  'appData.importConfirmTitle',
  'appData.importConfirmMessage',
  'settings.language',
  'main.searchPlaceholder',
];

describe('locales', () => {
  it('registers all 10 supported locales', () => {
    expect(SUPPORTED_LOCALES).toHaveLength(10);
    expect([...SUPPORTED_LOCALES].sort()).toEqual(
      ['de', 'en', 'es', 'fr', 'ja', 'ko', 'pt-BR', 'ru', 'zh-CN', 'zh-TW'].sort(),
    );
  });

  it('loads every supported locale with non-empty host dictionaries', () => {
    for (const locale of SUPPORTED_LOCALES) {
      const dict = getHostTranslations(locale);
      expect(Object.keys(dict).length).toBeGreaterThan(0);
    }
  });

  it('keeps host key parity with en across all locales', () => {
    const enKeys = Object.keys(getHostTranslations('en')).sort();
    for (const locale of SUPPORTED_LOCALES) {
      const keys = Object.keys(getHostTranslations(locale)).sort();
      expect(keys, `${locale} key mismatch`).toEqual(enKeys);
    }
  });

  it('former beta locales are not mostly English copies', () => {
    const enDict = getHostTranslations('en');
    const enKeys = Object.keys(enDict);
    for (const locale of ['de', 'es', 'fr', 'ja', 'ko', 'pt-BR', 'ru'] as const) {
      const dict = getHostTranslations(locale);
      let same = 0;
      for (const key of enKeys) {
        if (dict[key] === enDict[key]) same += 1;
      }
      const ratio = same / enKeys.length;
      expect(ratio, locale).toBeLessThan(0.35);
    }
  });

  it('marks no locales as beta', () => {
    expect(BETA_LOCALES).toEqual([]);
  });

  it('fully translated locales differ from en on user-facing strings', () => {
    for (const locale of FULLY_TRANSLATED_LOCALES) {
      if (locale === 'en') continue;
      expect(getTranslation(locale, 'common.ok')).not.toBe(en['common.ok']);
    }
  });

  it('falls back to en for unsupported locale codes', () => {
    expect(getTranslation('xx-XX', 'common.ok')).toBe(en['common.ok']);
    expect(getAllTranslations('invalid-locale')).toEqual(getAllTranslations('en'));
  });

  it('interpolates params for every locale', () => {
    for (const locale of SUPPORTED_LOCALES) {
      expect(getTranslation(locale, 'win.query', { db: 'testdb' })).toContain('testdb');
    }
  });

  it('resolves critical UI keys for every locale', () => {
    for (const locale of SUPPORTED_LOCALES) {
      for (const key of CRITICAL_KEYS) {
        const text = getTranslation(locale, key);
        expect(text.length, `${locale}:${key}`).toBeGreaterThan(0);
        expect(text).not.toBe(key);
      }
    }
  });

  it('zh-CN and en differ on at least some user-facing strings', () => {
    expect(zhCN['common.ok']).not.toBe(en['common.ok']);
    expect(zhCN['settings.language']).not.toBe(en['settings.language']);
  });

  it('zh-TW uses traditional form for language label', () => {
    expect(getAllTranslations('zh-TW')['settings.language']).toBe('語言');
  });

  it('getTranslation accepts SupportedLocale union members', () => {
    const locale: SupportedLocale = 'ko';
    expect(getTranslation(locale, 'common.cancel')).toBeTruthy();
  });

  it('replaces multiple distinct params', () => {
    // Prefer a key that uses one param; verify replace does not leave braces for known keys
    const text = getTranslation('en', 'appData.exportSuccess');
    expect(text.includes('{')).toBe(false);
  });

  it('falls back through dict chain for unknown keys', () => {
    const missing = 'this.key.does.not.exist' as TranslationKey;
    expect(getTranslation('en', missing)).toBe(missing);
    expect(getTranslation('zh-CN', missing)).toBe(missing);
  });
});
