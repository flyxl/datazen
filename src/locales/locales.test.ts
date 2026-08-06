import { describe, it, expect } from 'vitest';
import zhCN from './zh-CN';
import en from './en';
import {
  SUPPORTED_LOCALES,
  getAllTranslations,
  getTranslation,
  type SupportedLocale,
} from './index';

describe('locales', () => {
  it('registers all 10 supported locales', () => {
    expect(SUPPORTED_LOCALES).toHaveLength(10);
    expect([...SUPPORTED_LOCALES].sort()).toEqual(
      ['de', 'en', 'es', 'fr', 'ja', 'ko', 'pt-BR', 'ru', 'zh-CN', 'zh-TW'].sort(),
    );
  });

  it('loads every supported locale with non-empty dictionaries', () => {
    for (const locale of SUPPORTED_LOCALES) {
      const dict = getAllTranslations(locale);
      expect(Object.keys(dict).length).toBeGreaterThan(0);
    }
  });

  it('keeps key parity with zh-CN across all locales', () => {
    const zhKeys = Object.keys(zhCN).sort();
    for (const locale of SUPPORTED_LOCALES) {
      const keys = Object.keys(getAllTranslations(locale)).sort();
      expect(keys, `${locale} key mismatch`).toEqual(zhKeys);
    }
  });

  it('falls back to en for unsupported locale codes', () => {
    expect(getTranslation('xx-XX', 'common.ok')).toBe(en['common.ok']);
    expect(getAllTranslations('invalid-locale')).toEqual(getAllTranslations('en'));
  });

  it('interpolates params for every locale', () => {
    const sample: SupportedLocale = 'ja';
    expect(getTranslation(sample, 'win.query', { db: 'testdb' })).toContain('testdb');
  });
});
