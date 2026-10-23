#!/usr/bin/env node
/**
 * i18n-sync-check.mjs
 *
 * Compares English locale (en.ts) against all other locale files.
 * Reports missing keys and stale translations (unchanged since last tag).
 *
 * Two scopes, same contract (`en.ts` is the single source of truth):
 *   1. Host dictionaries — src/locales domain packs
 *   2. Driver-owned locale packs — packages/drivers/<id>/locales
 *      (driver packs self-register into the shared @datazen/ui registry, so
 *      nothing else validates their completeness)
 *
 * Usage:
 *   node scripts/i18n-sync-check.mjs              # check all locales
 *   node scripts/i18n-sync-check.mjs --from v1.0   # diff from tag v1.0
 *   node scripts/i18n-sync-check.mjs --verbose      # show changed en values
 *
 * The driver-pack scanner (`extractPackKeys`, `findDriverLocalePacks`,
 * `inspectDriverLocalePack`, `checkDriverLocalePacks`, `buildSyncReport`) and
 * the stale decision (`isSymbolOnlyValue`, `findStaleKeys`) are exported as
 * pure-ish functions and covered by
 * `scripts/__tests__/i18n-sync-check.test.mjs`; importing this module never
 * runs the CLI (same convention as `check-id-terminology.mjs`).
 */

import { execFileSync } from 'node:child_process';
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { resolve, dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = resolve(__dirname, '..');
const localesDir = resolve(root, 'src/locales');
const driversDir = resolve(root, 'packages/drivers');

const LOCALE_FILES = ['de', 'es', 'fr', 'ja', 'ko', 'pt-BR', 'ru', 'zh-TW', 'zh-CN'];

function extractKeys(filePath) {
  return extractHostKeysFromSources([readFileSync(filePath, 'utf-8')]);
}

/** Merge host dictionary keys parsed from one or more locale domain packs. */
export function extractHostKeysFromSources(sources) {
  const keys = {};
  const re = /^\s*'([^']+)':\s*(?:'((?:[^'\\]|\\.)*)'|"((?:[^"\\]|\\.)*)")/gm;
  for (const src of sources) {
    re.lastIndex = 0;
    let m;
    while ((m = re.exec(src)) !== null) {
      keys[m[1]] = m[2] ?? m[3] ?? '';
    }
  }
  return keys;
}

function extractLocaleKeys(locale) {
  const localeDir = resolve(localesDir, locale);
  const files = existsSync(localeDir)
    ? readdirSync(localeDir)
        .filter((name) => name.endsWith('.ts'))
        .sort()
        .map((name) => resolve(localeDir, name))
    : [resolve(localesDir, `${locale}.ts`)];
  const keys = {};
  for (const file of files) Object.assign(keys, extractKeys(file));
  return keys;
}

/**
 * Key extractor for driver-owned locale packs.
 *
 * Same shape as `extractKeys`, but additionally recognises the line-wrapped
 * form used by driver dictionaries (`'key':` ending a line, value on the next
 * one) — without it those keys would be invisible to the completeness check.
 *
 * Pure: takes the file *source*, so the extraction rules are unit-testable
 * (`scripts/__tests__/i18n-sync-check.test.mjs`).
 *
 * @param {string} src contents of a driver locale dictionary
 * @returns {Record<string, string>} key → value
 */
export function extractPackKeys(src) {
  const keys = {};
  const re = /^\s*'([^']+)':\s*(?:'((?:[^'\\]|\\.)*)'|"((?:[^"\\]|\\.)*)"|`(?:[^`\\]|\\.)*`|$)/gm;
  let m;
  while ((m = re.exec(src)) !== null) {
    keys[m[1]] = m[2] ?? m[3] ?? '';
  }
  return keys;
}

/** Read a driver pack dictionary via {@link extractPackKeys}. */
function readPackKeys(filePath) {
  return extractPackKeys(readFileSync(filePath, 'utf-8'));
}

/**
 * Every `packages/drivers/<id>/locales/` directory shipping an en.ts source.
 *
 * @param {string} dir absolute path of `packages/drivers`
 * @returns {Array<{ driver: string, dir: string }>} sorted by driver id
 */
export function findDriverLocalePacks(dir) {
  if (!existsSync(dir)) return [];
  const packs = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    if (!entry.isDirectory() || entry.name === 'node_modules') continue;
    const packDir = join(dir, entry.name, 'locales');
    if (existsSync(join(packDir, 'en.ts'))) packs.push({ driver: entry.name, dir: packDir });
  }
  return packs.sort((a, b) => a.driver.localeCompare(b.driver));
}

/**
 * Validate one driver locale pack: structural convention (an `index.ts`
 * side-effect module importing every language file, because that module *is*
 * the self-registration entry point) plus key completeness against `en.ts`.
 *
 * Pure reporting function — it returns the lines the CLI prints instead of
 * printing them, so both branches are unit-testable.
 *
 * @param {{ driver: string, dir: string }} pack as produced by {@link findDriverLocalePacks}
 * @returns {{ lines: string[], structural: number, missing: number }}
 */
export function inspectDriverLocalePack(pack) {
  const { driver, dir } = pack;
  const lines = [];
  let structural = 0;
  let missing = 0;

  const siblings = readdirSync(dir)
    .filter((name) => name.endsWith('.ts') && name !== 'en.ts' && name !== 'index.ts')
    .sort();

  // Structural convention: an index.ts side-effect module must import every
  // language file in the pack, otherwise the translations never reach the
  // shared registry at runtime.
  const indexPath = join(dir, 'index.ts');
  if (!existsSync(indexPath)) {
    lines.push(
      `[driver.${driver}] locales/index.ts is missing — a driver pack must self-register via registerTranslations().`,
    );
    structural += 1;
  } else {
    const indexSrc = readFileSync(indexPath, 'utf-8');
    const notRegistered = siblings.filter(
      (name) => !new RegExp(`from\\s+'\\./${name.replace(/\.ts$/, '')}'`).test(indexSrc),
    );
    if (notRegistered.length > 0) {
      lines.push(
        `[driver.${driver}] locales/index.ts does not import ${notRegistered.length} locale file(s): ${notRegistered.join(', ')}`,
      );
      structural += notRegistered.length;
    }
  }

  const packEnKeys = readPackKeys(join(dir, 'en.ts'));
  const packEnKeySet = new Set(Object.keys(packEnKeys));

  for (const file of siblings) {
    const locale = file.replace(/\.ts$/, '');
    const keys = readPackKeys(join(dir, file));
    const keySet = new Set(Object.keys(keys));
    const missingKeys = [...packEnKeySet].filter((k) => !keySet.has(k));
    const extra = [...keySet].filter((k) => !packEnKeySet.has(k));

    if (missingKeys.length === 0 && extra.length === 0) continue;

    lines.push(`[driver.${driver}/${locale}]`);
    if (missingKeys.length > 0) {
      lines.push(
        `  Missing ${missingKeys.length} key(s): ${missingKeys.slice(0, 10).join(', ')}${missingKeys.length > 10 ? '…' : ''}`,
      );
      missing += missingKeys.length;
    }
    if (extra.length > 0) {
      lines.push(
        `  Extra ${extra.length} key(s): ${extra.slice(0, 5).join(', ')}${extra.length > 5 ? '…' : ''}`,
      );
    }
  }

  return { lines, structural, missing };
}

/**
 * Scan every driver locale pack below `driversDir`.
 *
 * @param {string} driversDir absolute path of `packages/drivers`
 */
export function checkDriverLocalePacks(driversDir) {
  const packs = findDriverLocalePacks(driversDir);
  const lines = [];
  let structural = 0;
  let missing = 0;
  for (const pack of packs) {
    const result = inspectDriverLocalePack(pack);
    lines.push(...result.lines);
    structural += result.structural;
    missing += result.missing;
  }
  return { packCount: packs.length, lines, structural, missing };
}

/**
 * Final summary line(s) + process exit code. Structural driver-pack issues
 * fail the run even when no key is missing (an unregistered pack is worse
 * than an untranslated one).
 */
export function buildSyncReport({
  totalMissing,
  totalStale,
  totalStructural,
  driverPackCount,
  hostLocaleCount = LOCALE_FILES.length,
}) {
  if (totalMissing === 0 && totalStale === 0 && totalStructural === 0) {
    return { lines: ['All locale files are in sync with en.ts.'], exitCode: 0 };
  }
  return {
    lines: [
      `\nSummary: ${totalMissing} missing key(s), ${totalStale} stale translation(s) across ${hostLocaleCount} host locales; ` +
        `${totalStructural} driver pack issue(s) across ${driverPackCount} driver locale pack(s).`,
    ],
    exitCode: 1,
  };
}

/**
 * Placeholder token: `{name}` — braces, an identifier, no whitespace.
 *
 * Same shape as `extractPlaceholders` in `scripts/lib/i18nSync.mjs`; it is
 * duplicated here (not imported) because that shared module is owned by a
 * parallel track and must not be touched from this one.
 */
const PLACEHOLDER_TOKEN_RE = /\{[A-Za-z_][A-Za-z0-9_]*\}/g;

/**
 * Is this English value *purely symbolic* — i.e. does it carry no word at all
 * that a translator could have rendered differently?
 *
 * The rule is decided **by shape**, never by position:
 *
 *   1. strip every `{...}` placeholder token;
 *   2. what is left must be **non-empty** (an empty or all-whitespace value is
 *      NOT symbolic — it must not silence a real staleness signal);
 *   3. what is left must contain **no Unicode letter and no Unicode digit** —
 *      only symbols, punctuation and whitespace survive.
 *
 * So `≈ {count}` and `—` are symbolic, while `Sort`, `— unmapped —` and
 * `约 {count}` are not (they contain letters).
 *
 * Why this matters: a *new* key whose English value is a symbol must not force
 * the translator to invent a natural-language wording. `≈ {count}` was forced
 * into `ca. {count}` / `约 {count}` — a *worse* string than the original.
 *
 * @param {string} value the English value
 * @returns {boolean}
 */
export function isSymbolOnlyValue(value) {
  if (typeof value !== 'string') return false;
  const withoutPlaceholders = value.replace(PLACEHOLDER_TOKEN_RE, '');
  const rest = withoutPlaceholders.trim();
  // Empty / whitespace-only ⇒ NOT symbolic (see rule 2 above).
  if (rest.length === 0) return false;
  // A single letter or digit left over ⇒ a real word / number ⇒ not symbolic.
  return !/[\p{L}\p{N}]/u.test(rest);
}

/**
 * The stale decision.
 *
 * A translation is **stale** when its value is byte-identical to the English
 * one — the heuristic for "this string was never translated".
 *
 * EXCEPTION (the symbol exemption): when the English value is purely
 * symbolic (`≈ {count}`, `—`), an identical translation is **not** stale.
 * There is no wording to invent, and demanding one only degrades the value.
 *
 * The decision is made **by shape** of the English value
 * ({@link isSymbolOnlyValue}) — never by a positional heuristic such as
 * "the previous character is a quote", which is exactly what a previous
 * attempt at this toolchain did and why it failed.
 *
 * @param {Record<string, string>} enKeys English key → value
 * @param {Record<string, string>} localeKeys locale key → value
 * @param {Iterable<string>} changedEnKeys keys whose English changed/added
 * @returns {string[]} the stale keys, in the order of `changedEnKeys`
 */
export function findStaleKeys(enKeys, localeKeys, changedEnKeys) {
  const localeKeySet = new Set(Object.keys(localeKeys));
  const stale = [];
  for (const key of changedEnKeys) {
    if (!localeKeySet.has(key)) continue;
    if (localeKeys[key] !== enKeys[key]) continue;
    if (isSymbolOnlyValue(enKeys[key])) continue;
    stale.push(key);
  }
  return stale;
}

function getEnKeysAtRef(ref) {
  try {
    const paths = execFileSync(
      'git',
      ['ls-tree', '-r', '--name-only', ref, '--', 'src/locales/en'],
      {
        cwd: root,
        encoding: 'utf-8',
        stdio: ['pipe', 'pipe', 'pipe'],
      },
    )
      .split('\n')
      .filter((path) => path.endsWith('.ts'));
    const sourcePaths = paths.length > 0 ? paths : ['src/locales/en.ts'];
    const sources = sourcePaths.map((path) =>
      execFileSync('git', ['show', `${ref}:${path}`], {
        cwd: root,
        encoding: 'utf-8',
        stdio: ['pipe', 'pipe', 'pipe'],
      }),
    );
    return extractHostKeysFromSources(sources);
  } catch {
    return null;
  }
}

function getLatestTag() {
  try {
    return execFileSync('git', ['describe', '--tags', '--abbrev=0'], {
      cwd: root,
      encoding: 'utf-8',
    }).trim();
  } catch {
    return null;
  }
}

/**
 * CLI body. Everything reportable above it is an exported pure function, so
 * importing this module from `scripts/__tests__/i18n-sync-check.test.mjs` has
 * no side effects (the scanner used to run at module load and could there-
 * fore never be unit-tested).
 */
function runCli() {
  const args = process.argv.slice(2);
  const verbose = args.includes('--verbose');
  const fromIdx = args.indexOf('--from');
  const fromRef = fromIdx >= 0 ? args[fromIdx + 1] : getLatestTag();

  const enKeys = extractLocaleKeys('en');
  const enKeySet = new Set(Object.keys(enKeys));

  let changedEnKeys = new Set();
  if (fromRef) {
    const oldEnKeys = getEnKeysAtRef(fromRef);
    if (oldEnKeys) {
      for (const key of enKeySet) {
        if (!(key in oldEnKeys) || oldEnKeys[key] !== enKeys[key]) {
          changedEnKeys.add(key);
        }
      }
      console.log(
        `\nComparing the English locale against ${fromRef}: ${changedEnKeys.size} key(s) changed/added\n`,
      );
      if (verbose && changedEnKeys.size > 0) {
        for (const key of changedEnKeys) {
          const old = oldEnKeys[key];
          console.log(`  ${old ? '~' : '+'} ${key}`);
          if (old) console.log(`    was: ${old}`);
          console.log(`    now: ${enKeys[key]}`);
        }
        console.log();
      }
    } else {
      console.log(`\nCould not read en.ts at ref '${fromRef}', skipping diff.\n`);
    }
  }

  let totalMissing = 0;
  let totalStale = 0;

  for (const locale of LOCALE_FILES) {
    let localeKeys;
    try {
      localeKeys = extractLocaleKeys(locale);
    } catch {
      console.log(`[${locale}] locale files not found`);
      continue;
    }

    const localeKeySet = new Set(Object.keys(localeKeys));
    const missing = [...enKeySet].filter((k) => !localeKeySet.has(k));
    const extra = [...localeKeySet].filter((k) => !enKeySet.has(k));
    const stale = findStaleKeys(enKeys, localeKeys, changedEnKeys);

    if (missing.length === 0 && extra.length === 0 && stale.length === 0) {
      continue;
    }

    console.log(`[${locale}]`);
    if (missing.length > 0) {
      console.log(
        `  Missing ${missing.length} key(s): ${missing.slice(0, 10).join(', ')}${missing.length > 10 ? '…' : ''}`,
      );
      totalMissing += missing.length;
    }
    if (extra.length > 0) {
      console.log(
        `  Extra ${extra.length} key(s): ${extra.slice(0, 5).join(', ')}${extra.length > 5 ? '…' : ''}`,
      );
    }
    if (stale.length > 0) {
      console.log(
        `  Stale ${stale.length} key(s) (value equals en, may need translation): ${stale.slice(0, 10).join(', ')}${stale.length > 10 ? '…' : ''}`,
      );
      totalStale += stale.length;
    }
  }

  // Driver-owned locale packs (packages/drivers/<id>/locales/) — see
  // checkDriverLocalePacks(): the host aggregates nothing anymore, so this
  // guard is the only automated completeness check on a driver pack.
  const driverScan = checkDriverLocalePacks(driversDir);
  for (const line of driverScan.lines) console.log(line);
  totalMissing += driverScan.missing;

  const report = buildSyncReport({
    totalMissing,
    totalStale,
    totalStructural: driverScan.structural,
    driverPackCount: driverScan.packCount,
  });
  for (const line of report.lines) console.log(line);
  process.exitCode = report.exitCode;
}

/* istanbul ignore next — CLI seam: the pure functions above carry the unit tests */
if (process.argv[1] && process.argv[1].endsWith('i18n-sync-check.mjs')) {
  runCli();
}
