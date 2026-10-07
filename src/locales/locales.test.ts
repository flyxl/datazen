import { describe, it, expect, afterEach, beforeAll } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { relative, resolve } from 'node:path';
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

  it('resolves every literal t() key used in host source (en dictionary)', () => {
    // A key missing from en renders as its own name at runtime, in every
    // locale, and no other test can see it: the dictionaries only prove that
    // what IS in en is translated. Scan the call sites instead.
    const srcDir = resolve(__dirname, '..');
    const missing: Record<string, string[]> = {};
    const walk = (dir: string): void => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const full = resolve(dir, entry.name);
        if (entry.isDirectory()) {
          // Skip the dictionaries themselves and every test tree.
          if (entry.name !== 'locales' && entry.name !== '__tests__') walk(full);
          continue;
        }
        if (!/\.tsx?$/.test(entry.name) || entry.name.includes('.test.')) continue;
        for (const [, key] of readFileSync(full, 'utf8').matchAll(/\bt\(\s*'([\w.]+)'/g)) {
          if (key in en) continue;
          (missing[key] ??= []).push(relative(srcDir, full));
        }
      }
    };
    walk(srcDir);
    expect(missing).toEqual({});
  });

  it('sees driver packs that registered themselves in the shared registry', async () => {
    // Driver translations are owned by the driver package: importing its
    // locales entry mirrors what the app does when generated.ts loads the
    // driver UI meta. The host never aggregates driver keys itself.
    await import('../../packages/drivers/redis/locales');
    // Resolution contract only: which copy a driver key maps to belongs to the
    // driver's own en.ts, so no English value is pinned here.
    for (const key of ['redis.batchDelete', 'redis.console'] as const) {
      const fromSnapshot = getAllTranslations('en')[key];
      expect(fromSnapshot?.length, key).toBeGreaterThan(0);
      expect(fromSnapshot, key).not.toBe(key);
      expect(getTranslation('en', key), key).not.toBe(key);
    }
    expect(getTranslation('zh-CN', 'redis.console')).not.toBe('redis.console');
    // Absent from the host-only snapshot: the pack is driver-scoped.
    expect(getHostTranslations('en')['redis.batchDelete']).toBeUndefined();
  });

  it('keeps host document-view strings out of the driver `mongo.*` namespace', async () => {
    // The host and the mongodb driver both defined `mongo.*` until the host set
    // moved to `docView.*`. The driver pack self-registers *after* the host
    // (host at ./index module load, pack via ui/meta.ts → locales/index.ts on
    // every build that selects the driver), and `registerTranslations` merges
    // with `Object.assign` — so all 15 host values were overwritten and 9 of
    // the English strings DocumentConnectionView actually renders were
    // replaced ('Insert document' → 'Insert', '{count} document(s)' →
    // '{count} docs'). Asserting the host value survives a *later* driver
    // registration is what makes this a regression test rather than a
    // dictionary snapshot: before the rename this same assertion failed.
    await import('../../packages/drivers/mongodb/locales');

    const docViewKeys = (Object.keys(en) as string[]).filter((k) => k.startsWith('docView.'));
    expect(docViewKeys).toHaveLength(15);
    // Every one of the 19 t() call sites in DocumentConnectionView.tsx resolves
    // to a host-owned key, so no driver pack can reach them.
    for (const key of docViewKeys) {
      expect(getTranslation('en', key), key).toBe(en[key as TranslationKey]);
      expect(getTranslation('en', key), key).not.toBe(key);
    }
    // The host no longer defines any `mongo.*` key at all…
    expect((Object.keys(en) as string[]).filter((k) => k.startsWith('mongo.'))).toEqual([]);
    // …and the driver's own namespace is untouched by the host.
    expect(getTranslation('en', 'mongo.insert')).toBe('Insert');
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
    // Interpolation contract: the caller-supplied param lands in the resolved
    // copy and no `{placeholder}` residue remains. The surrounding wording is
    // owned by the dictionaries and is deliberately not pinned to a literal.
    expect(getTranslation('en', 'panel.closeTab', { title: 'Query' })).toContain('Query');
    expect(getTranslation('en', 'panel.closeTab', { title: 'Query' })).not.toContain('{');
    expect(getTranslation('zh-CN', 'panel.closeTab', { title: '查询' })).toContain('查询');
    expect(getTranslation('zh-CN', 'panel.closeTab', { title: '查询' })).not.toContain('{');
    expect(
      getTranslation('en', 'workflows.editor.parseError', { error: 'unexpected token' }),
    ).toContain('unexpected token');
  });

  it('contains snippet management keys in en', () => {
    // Presence + resolution (never the raw key), not the English wording.
    for (const key of [
      'query.snippets.add',
      'query.snippets.builtin',
      'query.snippets.syntaxGuideTitle',
      'query.snippets.prefixDuplicate',
    ] as const) {
      const text = getTranslation('en', key);
      expect(text.length, `en:${key}`).toBeGreaterThan(0);
      expect(text, `en:${key}`).not.toBe(key);
    }
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
