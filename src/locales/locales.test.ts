import { describe, it, expect, afterEach, beforeAll } from 'vitest';
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
  ensureAllLazyDomains,
  type TranslationKey,
} from './index';

const CRITICAL_KEYS: TranslationKey[] = [
  'common.ok',
  'common.cancel',
  'common.exportAppData',
  'common.importAppData',
  'common.exportAppData',
  'common.importAppData',
  'appData.exportSuccess',
  'common.importAppData',
  'appData.importConfirmMessage',
  'settings.language',
  'main.searchPlaceholder',
];

const UI_POLISH_KEYS: TranslationKey[] = [
  'common.dismiss',
  'common.discard',
  'common.operationFailed',
  'nav.modeRail',
  'settings.logLevel.trace',
  'settings.logLevel.debug',
  'settings.logLevel.info',
  'settings.logLevel.warn',
  'settings.logLevel.error',
  'settings.unsavedChangesTitle',
  'settings.unsavedChangesMessage',
  'settings.saveChanges',
  'settings.discardChanges',
  'dataTable.loading',
  'dataTable.empty',
  'dataTable.emptyHint',
  'dataTable.tableLabel',
  'workflows.editor.invalidYaml',
  'workflows.editor.parseError',
  'workflows.operationFailed',
  'select.noMatches',
  'select.toggleOptions',
  'errorBoundary.title',
  'errorBoundary.message',
  'errorBoundary.dismiss',
  'errorBoundary.reload',
  'panel.tabListLabel',
  'panel.closeTab',
  'permissions.contextConnections',
  'permissions.commandInvoke',
  'permissions.storageLocal',
  'permissions.uiNotify',
];

describe('locales', () => {
  // Lazy domain packs (sync/transfer/workflows/…) are not in the eager main chunk.
  // Preload them so getTranslation can resolve those keys in tests.
  beforeAll(async () => {
    await Promise.all(BUILTIN_LOCALES.map((locale) => ensureAllLazyDomains(locale)));
  });

  it('always exposes en as a built-in locale (fallback invariant)', () => {
    // en is the unconditional fallback dictionary; no generated/build issue
    // should ever drop it from BUILTIN_LOCALES.
    expect([...BUILTIN_LOCALES]).toContain('en');
    expect(BUILTIN_LOCALES.length).toBeGreaterThan(0);
  });

  it('loads every built-in locale with non-empty host dictionaries', () => {
    for (const locale of BUILTIN_LOCALES) {
      const dict = getHostTranslations(locale);
      expect(Object.keys(dict).length, `${locale} empty dict`).toBeGreaterThan(0);
    }
  });

  it('every built-in locale resolves every UI key without leaking a raw key', () => {
    const enKeys = Object.keys(getHostTranslations('en')) as TranslationKey[];
    for (const locale of BUILTIN_LOCALES) {
      for (const key of enKeys) {
        const text = getTranslation(locale, key);
        // Translated, or fell back to en — never the literal raw key. (Some
        // keys are intentionally the empty string, e.g. option separators.)
        expect(text, `${locale}:${key}`).not.toBe(key);
      }
    }
  });

  it('falls back to en for unsupported locale codes', () => {
    expect(getTranslation('xx-XX', 'common.ok')).toBe(en['common.ok']);
    expect(getAllTranslations('invalid-locale')).toEqual(getAllTranslations('en'));
  });

  it('sees driver packs that registered themselves in the shared registry', async () => {
    // Driver translations are owned by the driver package: importing its
    // locales entry mirrors what the app does when generated.ts loads the
    // driver UI meta. The host never aggregates driver keys itself.
    await import('../../packages/drivers/redis/locales');
    expect(getAllTranslations('en')['redis.batchDelete']).toBe('Delete selected');
    expect(getTranslation('en', 'redis.console')).toBe('Console');
    expect(getTranslation('zh-CN', 'redis.console')).not.toBe('redis.console');
    // Absent from the host-only snapshot: the pack is driver-scoped.
    expect(getHostTranslations('en')['redis.batchDelete']).toBeUndefined();
  });

  it('interpolates params for built-in locales', () => {
    for (const locale of BUILTIN_LOCALES) {
      expect(getTranslation(locale, 'win.query', { db: 'testdb' })).toContain('testdb');
    }
  });

  const SYNC_KEYS: TranslationKey[] = [
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
    'common.unsupportedPair',
    'sync.compare',
    'common.dataSyncTitle',
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

  it('resolves the v0.1.2 UI polish contract for en', () => {
    for (const key of UI_POLISH_KEYS) {
      const text = getTranslation('en', key);
      expect(text.length, `en:${key}`).toBeGreaterThan(0);
      expect(text, `en:${key}`).not.toBe(key);
    }
  });

  it('interpolates UI polish labels that include context', () => {
    expect(getTranslation('en', 'panel.closeTab', { title: 'Query' })).toBe('Close Query');
    expect(getTranslation('zh-CN', 'panel.closeTab', { title: '查询' })).toBe('关闭 查询');
    expect(
      getTranslation('en', 'workflows.editor.parseError', { error: 'unexpected token' }),
    ).toContain('unexpected token');
  });

  it('contains snippet management keys in en', () => {
    expect(getTranslation('en', 'query.snippets.add')).toBe('Add Snippet');
    expect(getTranslation('en', 'query.snippets.builtin')).toBe('Built-in');
    expect(getTranslation('en', 'query.snippets.syntaxGuideTitle')).toBe('Syntax Guide:');
    expect(getTranslation('en', 'query.snippets.prefixDuplicate')).toBe('Prefix already exists');
  });

  it('en contains user-facing fallback strings', () => {
    expect(en['common.ok']).toBeTruthy();
    expect(en['settings.language']).toBeTruthy();
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
      unregisterLocale('test-lang-empty');
    });

    it('registers and uses an extension locale', () => {
      registerLocale('test-lang', 'Test Language', { 'common.ok': 'TestOK' });
      expect(getAvailableLocales()).toContain('test-lang');
      expect(getTranslation('test-lang', 'common.ok')).toBe('TestOK');
    });

    it('falls back to en for missing keys in extension locale', () => {
      // Fresh locale code: the shared @datazen/ui registry is append-only per
      // key, so this must not reuse 'test-lang' from the previous case.
      registerLocale('test-lang-empty', 'Test Empty', {});
      expect(getTranslation('test-lang-empty', 'common.ok')).toBe(en['common.ok']);
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
