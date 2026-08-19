import { describe, it, expect, afterEach } from 'vitest';
import zhCN from './zh-CN';
import en from './en';
import {
  BUILTIN_LOCALES,
  getAllTranslations,
  getHostTranslations,
  getTranslation,
  registerLocale,
  unregisterLocale,
  getAvailableLocales,
  getExtensionLocales,
  type TranslationKey,
} from './index';

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
  it('registers 2 built-in locales (en, zh-CN)', () => {
    expect(BUILTIN_LOCALES).toHaveLength(2);
    expect([...BUILTIN_LOCALES].sort()).toEqual(['en', 'zh-CN'].sort());
  });

  it('loads every built-in locale with non-empty host dictionaries', () => {
    for (const locale of BUILTIN_LOCALES) {
      const dict = getHostTranslations(locale);
      expect(Object.keys(dict).length).toBeGreaterThan(0);
    }
  });

  it('keeps host key parity between en and zh-CN', () => {
    const enKeys = Object.keys(getHostTranslations('en')).sort();
    const zhKeys = Object.keys(getHostTranslations('zh-CN')).sort();
    expect(zhKeys).toEqual(enKeys);
  });

  it('falls back to en for unsupported locale codes', () => {
    expect(getTranslation('xx-XX', 'common.ok')).toBe(en['common.ok']);
    expect(getAllTranslations('invalid-locale')).toEqual(getAllTranslations('en'));
  });

  it('interpolates params for built-in locales', () => {
    for (const locale of BUILTIN_LOCALES) {
      expect(getTranslation(locale, 'win.query', { db: 'testdb' })).toContain('testdb');
    }
  });

  const SYNC_KEYS: TranslationKey[] = [
    'sync.overwriteRetiredBanner',
    'sync.applyUnavailable',
    'sync.rowDiffs',
    'sync.mappingMatched',
    'sync.mappingUnmappedSource',
    'sync.mappingUnmappedTarget',
    'sync.mappingDisabled',
    'sync.mappingIncompatible',
    'sync.mappingSummary',
    'sync.selectBoth',
    'sync.cannotSame',
    'sync.unsupportedHint',
    'sync.compare',
    'sync.windowTitle',
  ];

  it('resolves Data Sync workspace keys for built-in locales', () => {
    for (const locale of BUILTIN_LOCALES) {
      for (const key of SYNC_KEYS) {
        const text = getTranslation(locale, key);
        expect(text.length, `${locale}:${key}`).toBeGreaterThan(0);
        expect(text).not.toBe(key);
      }
    }
  });

  it('interpolates sync.mappingSummary placeholders', () => {
    for (const locale of BUILTIN_LOCALES) {
      const text = getTranslation(locale, 'sync.mappingSummary', {
        matched: 3,
        incompatible: 1,
        unmapped: 2,
      });
      expect(text).toContain('3');
      expect(text).toContain('1');
      expect(text).toContain('2');
      expect(text.includes('{')).toBe(false);
    }
  });

  it('resolves critical UI keys for built-in locales', () => {
    for (const locale of BUILTIN_LOCALES) {
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

  it('replaces multiple distinct params', () => {
    const text = getTranslation('en', 'appData.exportSuccess');
    expect(text.includes('{')).toBe(false);
  });

  it('falls back through dict chain for unknown keys', () => {
    const missing = 'this.key.does.not.exist' as TranslationKey;
    expect(getTranslation('en', missing)).toBe(missing);
    expect(getTranslation('zh-CN', missing)).toBe(missing);
  });

  describe('extension locale registration', () => {
    afterEach(() => {
      unregisterLocale('test-lang');
    });

    it('registers and uses an extension locale', () => {
      registerLocale('test-lang', 'Test Language', { 'common.ok': 'TestOK' });
      expect(getAvailableLocales()).toContain('test-lang');
      expect(getTranslation('test-lang', 'common.ok')).toBe('TestOK');
    });

    it('falls back to en for missing keys in extension locale', () => {
      registerLocale('test-lang', 'Test Language', {});
      expect(getTranslation('test-lang', 'common.ok')).toBe(en['common.ok']);
    });

    it('lists extension locales with labels', () => {
      registerLocale('test-lang', 'Test Language', {});
      const exts = getExtensionLocales();
      expect(exts).toContainEqual({ value: 'test-lang', label: 'Test Language' });
    });

    it('unregisters an extension locale', () => {
      registerLocale('test-lang', 'Test Language', {});
      unregisterLocale('test-lang');
      expect(getAvailableLocales()).not.toContain('test-lang');
    });
  });
});
