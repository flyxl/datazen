/** @vitest-environment node */
/**
 * Regression suite for `scripts/i18n-key-collision-check.mjs`.
 *
 * The defect this guards: the host and the mongodb driver both defined the
 * `mongo.*` namespace, the driver pack self-registers second, and
 * `registerTranslations()` merges with `Object.assign` — so all 15 host values
 * were silently replaced (9 of them with visibly shorter English). No existing
 * gate saw it: `i18n-sync-check.mjs` only compares one key set across languages
 * *within* a scope, never two scopes against each other.
 *
 * Every case below builds a throwaway tree, so the assertions are about the
 * scanner's behaviour, not about today's repo contents — with one exception
 * (`the real repository is collision-free`) which is the standing regression
 * assertion. That one would also pass against a scanner that finds nothing at
 * all, which is why the two fixture cases exist: they pin that a duplicate is
 * actually detected and named.
 */
import { describe, expect, it, afterEach, beforeEach } from 'vitest';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  checkI18nKeyCollisions,
  collectHostKeys,
  findKeyCollisions,
  hostDictionaryFiles,
  readDriverPackKeys,
} from '../i18n-key-collision-check.mjs';

const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '../..');

/** @typedef {Record<string, string>} Dict */

/** Dictionary source in the shape both host packs and driver packs ship. */
/** @param {Dict} entries */
const dictSource = (entries) =>
  `const locale = {\n${Object.entries(entries)
    .map(([k, v]) => `  '${k}': '${v}',`)
    .join('\n')}\n} as const;\n\nexport default locale;\n`;

/** `locales/index.ts` side-effect module — what findDriverLocalePacks looks for. */
const indexSource = () =>
  `import { registerTranslations } from '@datazen/ui';\n` +
  `import en from './en';\n\nregisterTranslations({ en });\n\nexport {};\n`;

describe('i18n-key-collision-check', () => {
  /** @type {string} */
  let root;

  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), 'i18n-collision-'));
  });

  afterEach(() => {
    rmSync(root, { recursive: true, force: true });
  });

  /**
   * Write one host locale dictionary under `src/locales/<locale>/`.
   *
   * @param {string} locale
   * @param {Dict} entries
   * @returns {string} the directory written
   */
  const writeHostLocale = (locale, entries) => {
    const dir = join(root, 'packages/frontend/src/locales', locale);
    mkdirSync(dir, { recursive: true });
    writeFileSync(join(dir, 'connection.ts'), dictSource(entries));
    return dir;
  };

  /**
   * Write one driver locale pack under `packages/drivers/<id>/locales/`.
   *
   * @param {string} driver
   * @param {Dict} enEntries
   * @param {Record<string, Dict>} [extraLocales]
   * @returns {string} the locales directory written
   */
  const writeDriverPack = (driver, enEntries, extraLocales = {}) => {
    const dir = join(root, 'packages/drivers', driver, 'locales');
    mkdirSync(dir, { recursive: true });
    writeFileSync(join(dir, 'en.ts'), dictSource(enEntries));
    for (const [code, entries] of Object.entries(extraLocales)) {
      writeFileSync(join(dir, `${code}.ts`), dictSource(entries));
    }
    writeFileSync(join(dir, 'index.ts'), indexSource());
    return dir;
  };

  /** @returns {ReturnType<typeof findKeyCollisions>} */
  const scan = () =>
    findKeyCollisions({
      localesDir: join(root, 'packages/frontend/src/locales'),
      driversDir: join(root, 'packages/drivers'),
    });

  /**
   * Collect the guard's output without letting it write to the test console.
   *
   * @returns {{ code: number, out: string, err: string }}
   */
  const runGuard = () => {
    /** @type {string[]} */
    const out = [];
    /** @type {string[]} */
    const err = [];
    const code = checkI18nKeyCollisions({
      root,
      /** @param {unknown} m */
      log: (m) => out.push(String(m)),
      /** @param {unknown} m */
      error: (m) => err.push(String(m)),
    });
    return { code, out: out.join('\n'), err: err.join('\n') };
  };

  describe('detects a duplicated key and names it', () => {
    it('reports the exact key when host and driver share one', () => {
      writeHostLocale('en', { 'docView.insert': 'Insert document', 'mongo.insert': 'Insert document' });
      writeDriverPack('mongodb', { 'mongo.insert': 'Insert' });

      const { collisions } = scan();
      expect(collisions.map((c) => c.key)).toEqual(['mongo.insert']);
      expect(collisions[0].driver).toBe('mongodb');
      expect(collisions[0].hostFile).toContain(
        join('packages/frontend/src/locales', 'en', 'connection.ts'),
      );
      expect(collisions[0].driverFile).toContain(join('mongodb', 'locales', 'en.ts'));
    });

    it('fails the gate with exit 1 and prints the offending key', () => {
      writeHostLocale('en', { 'docView.insert': 'Insert document', 'mongo.insert': 'Insert document' });
      writeDriverPack('mongodb', { 'mongo.insert': 'Insert' });

      const { code, err } = runGuard();
      expect(code).toBe(1);
      expect(err).toContain("'mongo.insert'");
      expect(err).toContain('FAILED: 1 key(s)');
    });

    it('detects a duplicate that only a NON-source-of-truth language defines', () => {
      // A key present only in de.ts still lands in the shared registry and can
      // still overwrite a host value, so reading en.ts alone would miss it.
      writeHostLocale('en', { 'mongo.deOnly': 'Host value' });
      writeDriverPack('mongodb', { 'mongo.other': 'Other' }, { de: { 'mongo.deOnly': 'Treiber' } });

      const { collisions } = scan();
      expect(collisions.map((c) => c.key)).toEqual(['mongo.deOnly']);
      // en.ts does not define this key, so the report names the file that does.
      expect(collisions[0].driverFile).toContain(join('mongodb', 'locales', 'de.ts'));
    });

    it('reports every colliding key, not just the first', () => {
      writeHostLocale('en', { 'mongo.a': 'A', 'mongo.b': 'B', 'mongo.c': 'C' });
      writeDriverPack('mongodb', { 'mongo.a': 'A2', 'mongo.b': 'B2', 'mongo.c': 'C2' });

      const { collisions } = scan();
      expect(collisions.map((c) => c.key)).toEqual(['mongo.a', 'mongo.b', 'mongo.c']);
      expect(runGuard().code).toBe(1);
    });

    it('finds a collision against any driver, not a hard-coded one', () => {
      writeHostLocale('en', { 'redis.collision': 'Host' });
      writeDriverPack('somefuture', { 'redis.collision': 'Driver' });

      const { collisions } = scan();
      expect(collisions).toHaveLength(1);
      expect(collisions[0].driver).toBe('somefuture');
    });
  });

  describe('driver x driver axis — one owner per key across every namespace', () => {
    // `registerTranslations` is Object.assign into ONE flat registry
    // (packages/ui/src/i18n.ts), so two drivers agreeing on a key is the same
    // silent-overwrite hazard as host-and-driver. On this repo the axis is
    // latent coverage: mongodb 15 keys x redis 417 keys intersect in 0.

    it('reports a key two driver packs both define, with no host involved', () => {
      writeHostLocale('en', { 'common.ok': 'OK' });
      writeDriverPack('alpha', { 'shared.toolbar.save': 'Save (alpha)' });
      writeDriverPack('beta', { 'shared.toolbar.save': 'Save (beta)' });

      const { collisions, packCollisions } = scan();
      // The host axis is clean — this is purely driver against driver.
      expect(collisions).toEqual([]);
      expect(packCollisions).toHaveLength(1);
      expect(packCollisions[0].key).toBe('shared.toolbar.save');
      expect(packCollisions[0].drivers).toEqual(['alpha', 'beta']);
    });

    it('fails the gate with exit 1 and names both drivers', () => {
      writeHostLocale('en', { 'common.ok': 'OK' });
      writeDriverPack('alpha', { 'shared.toolbar.save': 'Save (alpha)' });
      writeDriverPack('beta', { 'shared.toolbar.save': 'Save (beta)' });

      const { code, err } = runGuard();
      expect(code).toBe(1);
      expect(err).toContain("'shared.toolbar.save' is defined by BOTH the alpha and the beta driver");
      expect(err).toContain('more than one driver pack');
    });

    it('reports nothing when two drivers keep separate namespaces', () => {
      writeHostLocale('en', { 'common.ok': 'OK' });
      writeDriverPack('alpha', { 'alpha.toolbar.save': 'Save' });
      writeDriverPack('beta', { 'beta.toolbar.save': 'Save' });

      const { packCollisions } = scan();
      expect(packCollisions).toEqual([]);
      expect(runGuard().code).toBe(0);
    });

    it('compares every unordered pair, not just the first two packs', () => {
      writeHostLocale('en', { 'common.ok': 'OK' });
      writeDriverPack('a', { 'k.one': '1' });
      writeDriverPack('b', { 'k.two': '2' });
      writeDriverPack('c', { 'k.one': '1', 'k.two': '2' });

      const { packCollisions } = scan();
      // 3 pairs exist (a-b, a-c, b-c); only c collides, with each of the others.
      expect(packCollisions.map((c) => `${c.key}@${c.drivers.join('-')}`).sort()).toEqual([
        'k.one@a-c',
        'k.two@b-c',
      ]);
    });

    it('keeps the host axis reporting the same key independently', () => {
      // A key three ways must be reported on both axes, not merged into one row.
      writeHostLocale('en', { 'triple.owned': 'Host' });
      writeDriverPack('alpha', { 'triple.owned': 'Alpha' });
      writeDriverPack('beta', { 'triple.owned': 'Beta' });

      const { collisions, packCollisions } = scan();
      expect(collisions.map((c) => `${c.key}@${c.driver}`).sort()).toEqual([
        'triple.owned@alpha',
        'triple.owned@beta',
      ]);
      expect(packCollisions).toHaveLength(1);
      expect(runGuard().code).toBe(1);
    });
  });

  describe('separates namespaces cleanly', () => {
    it('reports nothing when the same words live under different namespaces', () => {
      // Exactly the post-fix shape: host `docView.insert` and driver
      // `mongo.insert` are the same UI action, owned by two different renderers.
      writeHostLocale('en', { 'docView.insert': 'Insert document' });
      writeDriverPack('mongodb', { 'mongo.insert': 'Insert' });

      const { collisions } = scan();
      expect(collisions).toEqual([]);
      const { code, out } = runGuard();
      expect(code).toBe(0);
      expect(out).toContain('0 key(s) owned by both');
    });

    it('unions the key set across all host locales', () => {
      writeHostLocale('en', { 'mongo.onlyInEn': 'x' });
      writeHostLocale('de', { 'mongo.onlyInEn': 'x', 'mongo.onlyInDe': 'y' });
      writeDriverPack('mongodb', { 'mongo.onlyInDe': 'z' });

      const { collisions, hostKeyCount } = scan();
      // Found even though `en` — the first locale scanned — does not define it.
      expect(collisions.map((c) => c.key)).toEqual(['mongo.onlyInDe']);
      expect(hostKeyCount).toBe(2);
    });
  });

  describe('positive control — an empty scan is never reported as clean', () => {
    it('exits 2 when there are no driver locale packs at all', () => {
      writeHostLocale('en', { 'mongo.insert': 'Insert document' });
      mkdirSync(join(root, 'packages/drivers'), { recursive: true });

      const { code, err } = runGuard();
      expect(code).toBe(2);
      expect(err).toContain('nothing to compare');
    });

    it('exits 2 when there are no host dictionaries at all', () => {
      mkdirSync(join(root, 'packages/frontend/src/locales'), { recursive: true });
      writeDriverPack('mongodb', { 'mongo.insert': 'Insert' });

      const { code, err } = runGuard();
      expect(code).toBe(2);
      expect(err).toContain('nothing to compare');
    });

    it('exits 2 when a discovered pack contributes zero keys', () => {
      // The half-empty scan. findDriverLocalePacks only requires locales/en.ts to
      // exist, so a pack holding `{}` is discovered and then never compared
      // against anything. Before this check the guard exited 0 and printed "ok"
      // with this pack listed as 0 — reporting success on a comparison it never
      // made. Reproduced before the fix, with exactly this fixture.
      writeHostLocale('en', { 'common.ok': 'OK' });
      writeDriverPack('alpha', { 'alpha.a': 'A' });
      writeDriverPack('ghost', {});

      const { packs } = scan();
      expect(packs).toHaveLength(2);
      expect(packs.find((p) => p.driver === 'ghost')?.keyCount).toBe(0);

      const { code, err } = runGuard();
      expect(code).toBe(2);
      expect(err).toContain('half-empty scan');
      expect(err).toContain('ghost');
      // It must NOT claim the repo is clean on the way out.
      expect(err).not.toContain('0 key(s) owned by both');
    });

    it('names every zero-key pack, not only the first', () => {
      writeHostLocale('en', { 'common.ok': 'OK' });
      writeDriverPack('alpha', { 'alpha.a': 'A' });
      writeDriverPack('ghost1', {});
      writeDriverPack('ghost2', {});

      const { code, err } = runGuard();
      expect(code).toBe(2);
      expect(err).toContain('2 driver pack(s) contributed 0');
      expect(err).toContain('ghost1, ghost2');
    });

    it('still exits 0 when every pack contributes keys', () => {
      // The half-empty check must not fire on a legitimate small pack.
      writeHostLocale('en', { 'common.ok': 'OK' });
      writeDriverPack('alpha', { 'alpha.a': 'A' });
      writeDriverPack('beta', { 'beta.b': 'B' });

      expect(runGuard().code).toBe(0);
    });

    it('prints the scanned input counts, so "0 collisions" is never bare', () => {
      writeHostLocale('en', { 'docView.insert': 'Insert document', 'common.ok': 'OK' });
      writeDriverPack('mongodb', { 'mongo.insert': 'Insert' });

      const { out } = runGuard();
      expect(out).toContain('2 host key(s)');
      expect(out).toContain('mongodb=1');
    });

    it('skips non-dictionary files at the src/locales root', () => {
      // builtinLocales.ts is generated and holds BUILTIN_LOCALE_LABELS, not
      // translations — reading it would invent keys nobody renders.
      const dir = join(root, 'packages/frontend/src/locales');
      mkdirSync(dir, { recursive: true });
      writeFileSync(
        join(dir, 'builtinLocales.ts'),
        "export const BUILTIN_LOCALE_LABELS = { en: 'English' };\n",
      );
      writeHostLocale('en', { 'common.ok': 'OK' });

      expect(hostDictionaryFiles(dir)).toEqual([]);
      expect(collectHostKeys(dir).keys.size).toBe(1);
    });
  });

  describe('readDriverPackKeys', () => {
    it('never counts locales/index.ts as a definition', () => {
      const dir = writeDriverPack('mongodb', { 'mongo.a': 'A' });
      const pack = readDriverPackKeys({ driver: 'mongodb', dir });
      // en.ts only — index.ts holds the import/registration side effect.
      expect(pack.fileCount).toBe(1);
      expect([...pack.keys.keys()]).toEqual(['mongo.a']);
    });

    it('names en.ts over another language when both define the key', () => {
      const dir = writeDriverPack('mongodb', { 'mongo.a': 'A' }, { de: { 'mongo.a': 'A-de' } });
      const pack = readDriverPackKeys({ driver: 'mongodb', dir });
      expect(pack.keys.get('mongo.a')).toContain(join('mongodb', 'locales', 'en.ts'));
    });
  });

  describe('the real repository', () => {
    it('has no key owned by both the host and a driver pack', () => {
      const result = findKeyCollisions({
        localesDir: join(REPO_ROOT, 'packages/frontend/src/locales'),
        driversDir: join(REPO_ROOT, 'packages/drivers'),
      });

      expect(
        result.collisions.map((c) => `${c.key} (${c.driver})`),
        'a translation key must be defined by exactly one owner',
      ).toEqual([]);
      // Guards the guard: a non-zero input on both sides is what makes the
      // empty collision list above a real result rather than an empty scan.
      expect(result.hostKeyCount).toBeGreaterThan(500);
      expect(result.hostLocaleCount).toBe(10);
      expect(result.packs.map((p) => p.driver).sort()).toEqual(['mongodb', 'redis']);
      for (const pack of result.packs) expect(pack.keyCount).toBeGreaterThan(0);
    });
  });
});
