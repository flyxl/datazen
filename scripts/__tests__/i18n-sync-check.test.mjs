/** @vitest-environment node */
/**
 * i18n-drivers BUG-002: the driver-locale-pack scanner of
 * `scripts/i18n-sync-check.mjs` is the ONLY automated completeness gate on a
 * driver pack (the host stopped aggregating driver dictionaries), so every branch
 * of it needs a permanent test — previously the ~100 added lines had 0 %
 * coverage because the script ran its report at module load and exported
 * nothing. Fixtures are written into a temp `packages/drivers` layout.
 */
import { describe, expect, it, afterEach, beforeEach } from 'vitest';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import {
  buildSyncReport,
  checkDriverLocalePacks,
  extractPackKeys,
  findDriverLocalePacks,
  inspectDriverLocalePack,
} from '../i18n-sync-check.mjs';

/** Dictionary source in the shape driver packs actually ship. */
const dictSource = (lines) => `const locale = {\n${lines.join('\n')}\n} as const;\n\nexport default locale;\n`;

/** locales/index.ts side-effect module importing the given locale codes. */
const indexSource = (codes) =>
  `import { registerTranslations } from '@datazen/ui';\n` +
  codes.map((c) => `import ${c.replace(/[^a-zA-Z]/g, '_')} from './${c}';`).join('\n') +
  `\n\nregisterTranslations({\n` +
  codes.map((c) => `  '${c}': ${c.replace(/[^a-zA-Z]/g, '_')},`).join('\n') +
  `\n});\n\nexport {};\n`;

describe('i18n-sync-check driver locale packs', () => {
  let root;
  let driversDir;

  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), 'i18n-sync-'));
    driversDir = join(root, 'packages/drivers');
    mkdirSync(driversDir, { recursive: true });
  });

  afterEach(() => {
    rmSync(root, { recursive: true, force: true });
  });

  /** Write one driver pack; `importedLocales` defaults to every locale file. */
  const writePack = (driver, { en, locales = {}, index = true, importedLocales }) => {
    const dir = join(driversDir, driver, 'locales');
    mkdirSync(dir, { recursive: true });
    writeFileSync(join(dir, 'en.ts'), dictSource(Object.entries(en).map(([k, v]) => `  '${k}': '${v}',`)));
    for (const [code, entries] of Object.entries(locales)) {
      writeFileSync(
        join(dir, `${code}.ts`),
        typeof entries === 'string' ? entries : dictSource(Object.entries(entries).map(([k, v]) => `  '${k}': '${v}',`)),
      );
    }
    if (index) {
      const codes = importedLocales ?? Object.keys(locales);
      writeFileSync(join(dir, 'index.ts'), indexSource(codes));
    }
    return dir;
  };

  describe('extractPackKeys', () => {
    it('reads inline single- and double-quoted values', () => {
      const keys = extractPackKeys(
        dictSource([
          "  'redis.a': 'Alpha',",
          '  \'redis.b\': "Beta",',
        ]),
      );
      expect(keys).toEqual({ 'redis.a': 'Alpha', 'redis.b': 'Beta' });
    });

    it('sees line-wrapped values (the driver dictionary style)', () => {
      // 'key': ends the line and the value lives on the next one — a plain
      // inline regex would report these keys as missing.
      const keys = extractPackKeys(
        `const locale = {\n  'redis.long':\n    'A very long translated value',\n  'redis.other': 'Other',\n} as const;\n`,
      );
      expect(Object.keys(keys).sort()).toEqual(['redis.long', 'redis.other']);
      expect(keys['redis.other']).toBe('Other');
      expect(keys['redis.long']).toBe('A very long translated value');
    });

    it('recognises template-literal values as present keys', () => {
      const keys = extractPackKeys(dictSource(["  'redis.tpl': `Hello {name}`,"]));
      expect(Object.keys(keys)).toEqual(['redis.tpl']);
    });

    it('ignores lines that are not dictionary entries', () => {
      const keys = extractPackKeys(
        `import x from './y';\nconst locale = {\n  // 'redis.comment': 'nope',\n  'redis.real': 'Real',\n};\nexport default locale;\n`,
      );
      expect(Object.keys(keys)).toEqual(['redis.real']);
    });
  });

  describe('findDriverLocalePacks', () => {
    it('returns nothing when the drivers directory does not exist', () => {
      expect(findDriverLocalePacks(join(root, 'nope'))).toEqual([]);
    });

    it('keeps only directories shipping locales/en.ts, sorted by driver id', () => {
      writePack('redis', { en: { 'redis.a': 'A' }, locales: { de: { 'redis.a': 'A' } } });
      writePack('mongodb', { en: { 'mongo.a': 'A' } });
      mkdirSync(join(driversDir, 'mysql'), { recursive: true }); // no locales/ at all
      mkdirSync(join(driversDir, 'node_modules', 'locales'), { recursive: true });
      writeFileSync(join(driversDir, 'node_modules', 'locales', 'en.ts'), 'x');
      const packs = findDriverLocalePacks(driversDir);
      expect(packs.map((p) => p.driver)).toEqual(['mongodb', 'redis']);
      expect(packs[1].dir).toBe(join(driversDir, 'redis', 'locales'));
    });
  });

  describe('inspectDriverLocalePack', () => {
    it('reports a clean pack with no issue and no output line', () => {
      const dir = writePack('redis', {
        en: { 'redis.a': 'A', 'redis.b': 'B' },
        locales: { de: { 'redis.a': 'A-de', 'redis.b': 'B-de' } },
      });
      expect(inspectDriverLocalePack({ driver: 'redis', dir })).toEqual({
        lines: [],
        structural: 0,
        missing: 0,
      });
    });

    it('counts a missing locales/index.ts as a structural issue', () => {
      const dir = writePack('redis', {
        en: { 'redis.a': 'A' },
        locales: { de: { 'redis.a': 'A-de' } },
        index: false,
      });
      const result = inspectDriverLocalePack({ driver: 'redis', dir });
      expect(result.structural).toBe(1);
      expect(result.lines.join('\n')).toContain(
        "[driver.redis] locales/index.ts is missing — a driver pack must self-register via registerTranslations().",
      );
    });

    it('lists locale files the index forgot to import', () => {
      const dir = writePack('redis', {
        en: { 'redis.a': 'A' },
        locales: { de: { 'redis.a': 'A-de' }, ja: { 'redis.a': 'A-ja' } },
        importedLocales: ['de'],
      });
      const result = inspectDriverLocalePack({ driver: 'redis', dir });
      expect(result.structural).toBe(1);
      expect(result.lines.join('\n')).toContain(
        '[driver.redis] locales/index.ts does not import 1 locale file(s): ja.ts',
      );
    });

    it('counts keys missing against en.ts and truncates the listing at 10', () => {
      const en = {};
      const de = {};
      for (let i = 0; i < 20; i += 1) en[`redis.k${i}`] = `V${i}`;
      for (let i = 0; i < 5; i += 1) de[`redis.k${i}`] = `v${i}`;
      const dir = writePack('redis', { en, locales: { de } });
      const result = inspectDriverLocalePack({ driver: 'redis', dir });
      const text = result.lines.join('\n');
      expect(result.missing).toBe(15);
      expect(result.structural).toBe(0);
      expect(text).toContain('[driver.redis/de]');
      expect(text).toContain('Missing 15 key(s):');
      expect(text.trimEnd().endsWith('…')).toBe(true);
    });

    it('lists extra keys without failing the run', () => {
      const dir = writePack('redis', {
        en: { 'redis.a': 'A' },
        locales: { de: { 'redis.a': 'A-de', 'redis.stale': 'Alt' } },
      });
      const result = inspectDriverLocalePack({ driver: 'redis', dir });
      expect(result.missing).toBe(0);
      expect(result.structural).toBe(0);
      expect(result.lines.join('\n')).toContain('Extra 1 key(s): redis.stale');
    });

    it('does not report false missing keys for line-wrapped dictionaries', () => {
      const dir = writePack('redis', {
        en: { 'redis.a': 'A', 'redis.b': 'B' },
        locales: {
          de: dictSource(["  'redis.a':", "    'A long German value',", "  'redis.b': 'B-de',"]),
        },
      });
      expect(inspectDriverLocalePack({ driver: 'redis', dir }).missing).toBe(0);
    });
  });

  describe('checkDriverLocalePacks', () => {
    it('aggregates every pack below the drivers directory', () => {
      writePack('redis', {
        en: { 'redis.a': 'A' },
        locales: { de: {} },
      });
      writePack('mongodb', {
        en: { 'mongo.a': 'A' },
        locales: { de: { 'mongo.a': 'A-de' } },
        index: false,
      });
      const result = checkDriverLocalePacks(driversDir);
      expect(result.packCount).toBe(2);
      expect(result.missing).toBe(1);
      expect(result.structural).toBe(1);
      expect(result.lines.join('\n')).toContain('[driver.mongodb] locales/index.ts is missing');
      expect(result.lines.join('\n')).toContain('Missing 1 key(s): redis.a');
    });

    it('is empty when no driver ships a locale pack', () => {
      mkdirSync(join(driversDir, 'mysql'), { recursive: true });
      expect(checkDriverLocalePacks(driversDir)).toEqual({
        packCount: 0,
        lines: [],
        structural: 0,
        missing: 0,
      });
    });
  });

  describe('buildSyncReport', () => {
    it('passes on a fully synced repository with exit code 0', () => {
      expect(buildSyncReport({ totalMissing: 0, totalStale: 0, totalStructural: 0, driverPackCount: 2 })).toEqual({
        lines: ['All locale files are in sync with en.ts.'],
        exitCode: 0,
      });
    });

    it('fails on missing keys and names both host and driver scopes', () => {
      const report = buildSyncReport({ totalMissing: 3, totalStale: 0, totalStructural: 0, driverPackCount: 2 });
      expect(report.exitCode).toBe(1);
      expect(report.lines[0].trim()).toBe(
        'Summary: 3 missing key(s), 0 stale translation(s) across 8 host locales; 0 driver pack issue(s) across 2 driver locale pack(s).',
      );
    });

    it('fails on structural issues even with zero missing keys', () => {
      const report = buildSyncReport({ totalMissing: 0, totalStale: 0, totalStructural: 1, driverPackCount: 0 });
      expect(report.exitCode).toBe(1);
      expect(report.lines[0]).toContain('1 driver pack issue(s) across 0 driver locale pack(s).');
    });

    it('keeps the stale-only case failing (host translation debt)', () => {
      expect(
        buildSyncReport({ totalMissing: 0, totalStale: 5, totalStructural: 0, driverPackCount: 1, hostLocaleCount: 2 })
          .exitCode,
      ).toBe(1);
    });
  });
});
