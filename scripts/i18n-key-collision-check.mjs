#!/usr/bin/env node
/**
 * i18n-key-collision-check.mjs — host ↔ driver translation-key ownership guard.
 *
 * WHY (measured, not assumed — see the numbers in the closing report):
 * the host and every driver feed the SAME translation registry
 * (`@datazen/ui` `registerTranslations`, which merges with `Object.assign`).
 * Nothing in that engine has namespaces: a key is a flat string, and on a
 * duplicate the LAST registration silently wins. `i18n-sync-check.mjs` only
 * compares one key set across languages within a scope — it never compares two
 * scopes against each other, so a cross-scope duplicate is invisible to it.
 *
 * That is exactly how the host's `mongo.*` set (src/locales/<loc>/connection.ts,
 * consumed by 19 changed lines in src/windows/connection/DocumentConnectionView.tsx
 * holding 21 `t()` calls over 15 distinct keys — lines 369 and 604 carry two calls
 * each) and the mongodb driver pack's `mongo.*` set (packages/drivers/mongodb/locales/*)
 * ended up defining the same 15 keys. The driver pack self-registers second
 * (host registers at src/locales/index.ts module load; the pack registers via
 * packages/drivers/<id>/ui/meta.ts → locales/index.ts, imported later through
 * src/extensions/generated.ts), so 15/15 host values were overwritten and 9/15
 * English strings the host actually renders were replaced by the driver's
 * wording ('Insert document' → 'Insert', '{count} document(s)' → '{count} docs').
 * The two sides serve different consumers — the host renders the document
 * connection view, the driver ships the meta record and a JSON-command builder
 * — so merging the wording is not a fix either. The only correct fix is
 * separate namespaces, which this script then keeps separate.
 *
 * THE RULE: a translation key must have exactly one owner. That has to hold
 * across every participating namespace, not just the host/driver axis: driver
 * packs self-register into the same flat registry, so a key defined by two
 * different drivers is the same class of silent-overwrite hazard. Two axes are
 * therefore checked: host × driver, and driver × driver. On this tree the
 * second axis is latent coverage, not a live conflict (mongodb 15 keys ×
 * redis 417 keys intersect in 0).
 *
 * WHAT THIS GUARD DOES NOT DO — registration order. It is a pure key-set
 * comparison and has no model of import order at all, so when there is no
 * duplicate it says nothing about who would win. That blindness is not
 * hypothetical: measured on the pre-fix tree through the real pipeline,
 * driverWins=15 and hostWins=0 — the host value was never rendered even once.
 * If two owners ever need the same key with no rename, that is an engine
 * change (namespaced registration), not something this script can catch.
 *
 * The OK line deliberately prints how many host keys and how many keys per
 * driver pack were actually read. "0 collisions" is only meaningful next to
 * non-zero input counts — a scanner that silently scanned nothing also reports
 * zero. Two empty-scan shapes are refused with exit 2 rather than 0: nothing
 * scanned at all, and a HALF-empty scan where a discovered pack contributed
 * zero keys and so was never actually compared against anything.
 *
 * Usage:
 *   node scripts/i18n-key-collision-check.mjs
 *   node scripts/i18n-key-collision-check.mjs --root=<dir>
 *
 * Exit codes: 0 no collision · 1 at least one collision · 2 scan incomplete.
 */
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { extractPackKeys, findDriverLocalePacks } from './i18n-sync-check.mjs';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');

/**
 * Every host locale directory under `packages/frontend/src/locales`. `en` + `zh-CN` are the two
 * runtime-registered locales (src/locales/builtinLocales.ts); the other eight
 * ship as source-only files but are equally part of the host key contract, so a
 * key present in any of them counts as host-owned.
 */
export const HOST_LOCALE_DIRS = [
  'en',
  'zh-CN',
  'zh-TW',
  'de',
  'es',
  'fr',
  'ja',
  'ko',
  'pt-BR',
  'ru',
];

/** Files at `packages/frontend/src/locales/` root that are NOT dictionaries (generated, barrels). */
const HOST_SKIP_FILES = new Set([
  'builtinLocales.ts', // codegen: BUILTIN_LOCALES + labels, not a dictionary
  'builtin-locales.json',
  'domains.ts',
  'index.ts',
  'lazyPacks.ts',
  't.ts',
  'fullLocales.ts',
]);

const TAG = '[i18n-key-collision]';

/**
 * @param {string} dir absolute path of a host locale directory
 * @returns {string[]} absolute paths of its dictionary files, sorted
 */
export function hostDictionaryFiles(dir) {
  if (!existsSync(dir)) return [];
  return readdirSync(dir)
    .filter((name) => name.endsWith('.ts') && !HOST_SKIP_FILES.has(name))
    .sort()
    .map((name) => join(dir, name));
}

/**
 * Read one host locale into `key → defining file`.
 *
 * @param {string} locale
 * @param {string} localesDir absolute path of `packages/frontend/src/locales`
 * @returns {Map<string, string>} key → absolute file path that defines it
 */
export function readHostLocaleKeys(locale, localesDir) {
  const owners = new Map();
  for (const file of hostDictionaryFiles(join(localesDir, locale))) {
    for (const key of Object.keys(extractPackKeys(readFileSync(file, 'utf-8')))) {
      // First definition wins for reporting; i18n-sync-check.mjs is the gate on
      // "one definition per locale", this gate only cares about the owner side.
      if (!owners.has(key)) owners.set(key, file);
    }
  }
  return owners;
}

/**
 * Collect every host-owned key with the file that defines it.
 *
 * @param {string} localesDir absolute path of `packages/frontend/src/locales`
 * @returns {{ keys: Map<string, string>, localeCount: number, fileCount: number }}
 */
export function collectHostKeys(localesDir) {
  const keys = new Map();
  let localeCount = 0;
  let fileCount = 0;
  for (const locale of HOST_LOCALE_DIRS) {
    const files = hostDictionaryFiles(join(localesDir, locale));
    if (files.length === 0) continue;
    localeCount += 1;
    fileCount += files.length;
    for (const [key, file] of readHostLocaleKeys(locale, localesDir)) {
      if (!keys.has(key)) keys.set(key, file);
    }
  }
  return { keys, localeCount, fileCount };
}

/**
 * Read one driver locale pack into `key → defining file`.
 *
 * Every language file of the pack is read, not just `en.ts`: a key that only
 * one language of a pack carries is still a definition that will land in the
 * shared registry and can still overwrite a host value.
 *
 * `en.ts` is the pack's source of truth, so it is the file named in a report
 * even when another language of the pack also defines the key.
 *
 * @param {{ driver: string, dir: string }} pack as produced by findDriverLocalePacks
 * @returns {{ keys: Map<string, string>, fileCount: number }}
 */
export function readDriverPackKeys(pack) {
  const keys = new Map();
  const files = readdirSync(pack.dir)
    .filter((name) => name.endsWith('.ts') && name !== 'index.ts')
    .sort();
  for (const name of files) {
    const file = join(pack.dir, name);
    const isSource = name === 'en.ts';
    for (const key of Object.keys(extractPackKeys(readFileSync(file, 'utf-8')))) {
      if (isSource || !keys.has(key)) keys.set(key, file);
    }
  }
  return { keys, fileCount: files.length };
}

/**
 * Find every key defined by more than one owner.
 *
 * TWO AXES, because the shared registry has no namespaces and the owners are
 * more than two:
 *
 *  1. host × driver — the host dictionary against each driver locale pack.
 *  2. driver × driver — each pack against every other pack. `registerTranslations`
 *     is `Object.assign(target, dict)` into ONE flat registry
 *     (packages/ui/src/i18n.ts), so two driver packs that agree on a key are the
 *     same class of runtime hazard as host-and-driver: whichever self-registers
 *     second silently wins, and neither pack can see the other at author time.
 *     Measured on this tree: mongodb(15) x redis(417) intersect in 0 keys, so
 *     axis 2 is LATENT COVERAGE, not a live conflict. The criterion being
 *     enforced is "a key has exactly one owner across every participating
 *     namespace", not "the host does not collide with a driver".
 *
 * @param {{ localesDir: string, driversDir: string }} dirs
 * @returns {{ collisions: Array<{ key: string, driver: string, hostFile: string, driverFile: string }>,
 *             packCollisions: Array<{ key: string, drivers: [string, string], files: [string, string] }>,
 *             hostKeyCount: number, hostLocaleCount: number, hostFileCount: number,
 *             packs: Array<{ driver: string, keyCount: number, fileCount: number }> }}
 */
export function findKeyCollisions({ localesDir, driversDir }) {
  const host = collectHostKeys(localesDir);
  const collisions = [];
  const packCollisions = [];
  const packs = [];
  /** @type {Array<{ driver: string, keys: Map<string, string> }>} */
  const loaded = [];

  for (const pack of findDriverLocalePacks(driversDir)) {
    const driver = readDriverPackKeys(pack);
    loaded.push({ driver: pack.driver, keys: driver.keys });
    packs.push({
      driver: pack.driver,
      keyCount: driver.keys.size,
      fileCount: driver.fileCount,
    });
    for (const [key, driverFile] of driver.keys) {
      const hostFile = host.keys.get(key);
      if (hostFile !== undefined) {
        collisions.push({ key, driver: pack.driver, hostFile, driverFile });
      }
    }
  }

  // Axis 2. Every unordered pair of discovered packs, compared key by key.
  // `findDriverLocalePacks` returns them sorted by driver name, so index order
  // is a stable, deterministic pair order and the report is reproducible.
  for (let i = 0; i < loaded.length; i += 1) {
    for (let j = i + 1; j < loaded.length; j += 1) {
      const a = loaded[i];
      const b = loaded[j];
      for (const [key, aFile] of a.keys) {
        const bFile = b.keys.get(key);
        if (bFile !== undefined) {
          packCollisions.push({ key, drivers: [a.driver, b.driver], files: [aFile, bFile] });
        }
      }
    }
  }

  collisions.sort(
    (a, b) => a.key.localeCompare(b.key) || a.driver.localeCompare(b.driver),
  );
  packCollisions.sort(
    (a, b) => a.key.localeCompare(b.key) || a.drivers[0].localeCompare(b.drivers[0]),
  );
  return {
    collisions,
    packCollisions,
    hostKeyCount: host.keys.size,
    hostLocaleCount: host.localeCount,
    hostFileCount: host.fileCount,
    packs,
  };
}

/**
 * @typedef {{ root?: string, log?: Function, error?: Function }} GuardOptions
 */

/**
 * @param {GuardOptions} [opts]
 * @returns {number} exit code (see header)
 */
export function checkI18nKeyCollisions(opts = {}) {
  const root = opts.root ?? ROOT;
  const log = opts.log ?? console.log.bind(console);
  const error = opts.error ?? console.error.bind(console);

  const result = findKeyCollisions({
    localesDir: join(root, 'packages/frontend/src/locales'),
    driversDir: join(root, 'packages/drivers'),
  });

  // Positive control: refuse to report success if either side came back empty.
  // Without this, a renamed directory or a broken checkout would look exactly
  // like a clean repo.
  if (result.hostKeyCount === 0 || result.packs.length === 0) {
    error(
      `${TAG} FAILED: nothing to compare — read ${result.hostKeyCount} host key(s) from ` +
        `${result.hostLocaleCount} host locale(s) and found ${result.packs.length} driver locale pack(s) ` +
        `(root=${root}). Refusing to report "0 collisions" from an empty scan.`,
    );
    return 2;
  }

  // Positive control, second half: a HALF-empty scan is just as untrustworthy as
  // a wholly empty one. `findDriverLocalePacks` only requires that a pack have
  // `locales/en.ts`; that file can be an empty object, or hold only non-string
  // entries the extractor skips. A pack that contributes zero keys was not
  // actually compared against anything, yet the old check passed it through and
  // printed "0 collisions" — the guard reporting success on a comparison it
  // never made. Measured before this check existed: a two-pack fixture where
  // one pack contributed 0 keys exited 0 with "ok".
  const silentPacks = result.packs.filter((p) => p.keyCount === 0);
  if (silentPacks.length > 0) {
    error(
      `${TAG} FAILED: half-empty scan — ${silentPacks.length} driver pack(s) contributed 0 ` +
        `translation key(s): ${silentPacks.map((p) => p.driver).join(', ')}.`,
    );
    error(
      `    A pack is counted as present by findDriverLocalePacks when it has a locales/en.ts, but an ` +
        `empty one is never compared against the host, so "0 collisions" from it is a false negative.`,
    );
    error(
      `    Either the pack is a stub that should be removed, or its keys stopped parsing. Fix that ` +
        `before trusting this guard's result.`,
    );
    return 2;
  }

  for (const hit of result.collisions) {
    error(
      `${TAG} '${hit.key}' is defined by BOTH the host and the ${hit.driver} driver ` +
        `(host: ${hit.hostFile}, driver: ${hit.driverFile}).`,
    );
    error(
      `    The shared registry merges with Object.assign, so the ${hit.driver} pack — which registers ` +
        `after the host — silently wins and the host never renders its own string.`,
    );
    error(
      `    Rename one side into its own namespace (host connection-view strings live under ` +
        `'docView.*'; driver packs own theirs).`,
    );
  }

  for (const hit of result.packCollisions) {
    const [first, second] = hit.drivers;
    error(
      `${TAG} '${hit.key}' is defined by BOTH the ${first} and the ${second} driver ` +
        `(${first}: ${hit.files[0]}, ${second}: ${hit.files[1]}).`,
    );
    error(
      `    Driver packs self-register into the SAME flat registry, so whichever imports second wins ` +
        `and the other pack never renders its own string.`,
    );
    error(`    Give each driver its own namespace, e.g. '${first}.' / '${second}.' prefixes.`);
  }

  const packSummary = result.packs
    .map((p) => `${p.driver}=${p.keyCount}`)
    .join(' ');
  if (result.collisions.length > 0) {
    error(
      `${TAG} FAILED: ${result.collisions.length} key(s) defined by both the host and a driver pack.`,
    );
    return 1;
  }
  if (result.packCollisions.length > 0) {
    error(
      `${TAG} FAILED: ${result.packCollisions.length} key(s) defined by more than one driver pack.`,
    );
    return 1;
  }

  log(
    `${TAG} ok — ${result.hostKeyCount} host key(s) from ${result.hostLocaleCount} locale(s) ` +
      `(${result.hostFileCount} files) vs ${result.packs.length} driver pack(s) [${packSummary}]: ` +
      `0 key(s) owned by both, on both axes (host x driver, driver x driver).`,
  );
  return 0;
}

/**
 * @param {{ argv?: string[] } & GuardOptions} [opts]
 * @returns {number} exit code
 */
export function runCli(opts = {}) {
  const { argv = process.argv, ...rest } = opts;
  const rootArg = argv.find((arg) => arg.startsWith('--root='));
  if (rootArg) rest.root = resolve(rootArg.slice('--root='.length));
  return checkI18nKeyCollisions(rest);
}

/* istanbul ignore next */
if (process.argv[1] && process.argv[1].endsWith('i18n-key-collision-check.mjs')) {
  process.exitCode = runCli();
}
