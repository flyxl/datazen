#!/usr/bin/env node
/**
 * Audit the locale domain packs against the English source of truth.
 *
 * This is the **audit** half of the i18n sync toolchain:
 *   · `scripts/i18n-sync-check.mjs`  — the release blocker (missing keys);
 *   · `scripts/i18n-sync-apply.mjs`  — writes a batch into the packs;
 *   · `scripts/i18n-sync-verify.mjs` — this file: the self-check that has to
 *     pass before a batch is committed.
 *
 * ## What it checks
 *
 * 1. **Key-set parity** — every locale carries exactly the English key set.
 * 2. **Placeholder parity** — for every key, the SET of `{name}` placeholder
 *    names in the translation equals the English set. Names may be reordered,
 *    but not renamed, dropped, or invented. `${1:tab}` tab stops and JSON
 *    braces are NOT placeholders and are excluded (see `extractPlaceholders`).
 * 3. **Untranslated residue** — values byte-identical to the English. The
 *    report is the raw list plus a delta against a baseline ref, because the
 *    number that matters is "did THIS change introduce any".
 * 4. **Entry integrity** — no key declaration was split from its value (the
 *    failure mode the applier's anchor bug produced).
 *
 * ## Exit codes
 *
 * `0` clean · `1` problems found · `2` bad usage.
 *
 * ## Usage
 *
 * ```bash
 * node scripts/i18n-sync-verify.mjs [--locale=<locale>] [--root=<path>] [--baseline=<ref>]
 * node scripts/i18n-sync-verify.mjs --help
 * ```
 */
import { execFileSync } from 'node:child_process';
import { readFileSync, readdirSync, existsSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  LOCALE_FILES,
  extractPlaceholders,
  parseEntries,
  parsePack,
  samePlaceholders,
} from './lib/i18nSync.mjs';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, '..');

const USAGE = `Usage: node scripts/i18n-sync-verify.mjs [options]

Audit every non-English locale against the English source of truth.

Options:
  --locale=<locale>   audit only this locale        (default: all ${LOCALE_FILES.length})
  --root=<path>       repository root               (default: the repo)
  --baseline=<ref>    git ref to diff the
                       byte-identical count against (default: HEAD)
  --help              print this help

Checks: key-set parity · placeholder-name parity · byte-identical residue
(plus its delta against the baseline) · entry integrity.

Exit codes: 0 clean · 1 problems found · 2 bad usage.`;

function parseArgs(argv) {
  const args = { root: ROOT, help: false, locale: null, baseline: 'HEAD' };
  for (const raw of argv) {
    let matched = false;
    if (raw === '--help' || raw === '-h') {
      args.help = true;
      continue;
    }
    for (const [flag, key] of [
      ['--locale=', 'locale'],
      ['--root=', 'root'],
      ['--baseline=', 'baseline'],
    ]) {
      if (raw.startsWith(flag)) {
        args[key] = raw.slice(flag.length);
        matched = true;
        break;
      }
    }
    if (matched) continue;
    throw new Error(`unknown argument: ${raw}`);
  }
  return args;
}

/**
 * The key → value map of a locale at a git ref.
 *
 * @param {string} root
 * @param {string} ref
 * @param {string} locale
 * @returns {Record<string, string>}
 */
export function packAtRef(root, ref, locale) {
  let files;
  try {
    files = execFileSync('git', ['ls-tree', '-r', '--name-only', ref, '--', `src/locales/${locale}`], {
      cwd: root,
      encoding: 'utf-8',
    })
      .split('\n')
      .filter((p) => p.endsWith('.ts'));
  } catch {
    return {};
  }
  const out = {};
  for (const f of files) {
    const source = execFileSync('git', ['show', `${ref}:${f}`], { cwd: root, encoding: 'utf-8' });
    Object.assign(out, parsePack(source));
  }
  return out;
}

/**
 * Audit one locale.
 *
 * @param {string} root
 * @param {string} locale
 * @param {string} baseline
 * @returns {{ problems: number, report: object }}
 */
export function verifyLocale(root, locale, baseline) {
  const enDir = resolve(root, 'src/locales/en');
  const localeDir = resolve(root, 'src/locales', locale);
  const en = {};
  const enFiles = readdirSync(enDir).filter((f) => f.endsWith('.ts')).sort();
  for (const f of enFiles) Object.assign(en, parsePack(readFileSync(resolve(enDir, f), 'utf-8')));

  const local = {};
  const localFiles = existsSync(localeDir)
    ? readdirSync(localeDir).filter((f) => f.endsWith('.ts')).sort()
    : [];
  for (const f of localFiles) {
    Object.assign(local, parsePack(readFileSync(resolve(localeDir, f), 'utf-8')));
  }

  const enKeys = Object.keys(en);
  const localKeys = Object.keys(local);
  const missing = enKeys.filter((k) => !(k in local));
  const extra = localKeys.filter((k) => !(k in en));

  const placeholderMismatches = [];
  for (const k of enKeys) {
    if (!(k in local)) continue;
    if (!samePlaceholders(en[k], local[k])) {
      placeholderMismatches.push({
        key: k,
        en: [...extractPlaceholders(en[k])],
        loc: [...extractPlaceholders(local[k])],
      });
    }
  }

  const identical = enKeys.filter((k) => k in local && local[k] === en[k]);
  const baseLocal = packAtRef(root, baseline, locale);
  const baseEn = packAtRef(root, baseline, 'en');
  const baseIdentical = new Set(
    Object.keys(baseEn).filter((k) => k in baseLocal && baseLocal[k] === baseEn[k]),
  );
  const newlyIdentical = identical.filter((k) => !baseIdentical.has(k));

  // Entry integrity: a key whose value lives on the NEXT line must have a
  // value there, not another key's declaration.
  const corrupt = [];
  for (const f of localFiles) {
    const lines = readFileSync(resolve(localeDir, f), 'utf-8').split('\n');
    lines.forEach((line, i) => {
      const m = /^\s*'([^']+)':\s*$/.exec(line);
      if (!m) return;
      const next = (lines[i + 1] ?? '').trim();
      const isValue = /^(['"])/.test(next) && !/^'[^']+':/.test(next);
      if (!isValue) corrupt.push({ file: f, line: i + 1, key: m[1] });
    });
  }

  const problems = missing.length + extra.length + placeholderMismatches.length + newlyIdentical.length + corrupt.length;
  return {
    problems,
    report: {
      locale,
      keyCount: localKeys.length,
      missing,
      extra,
      placeholderMismatches,
      identicalCount: identical.length,
      newlyIdentical,
      corrupt,
    },
  };
}

export function runCli({ argv = process.argv.slice(2) } = {}) {
  const out = (s) => process.stdout.write(`${s}\n`);
  const err = (s) => process.stderr.write(`${s}\n`);
  let args;
  try {
    args = parseArgs(argv);
  } catch (e) {
    err(String(e.message));
    err(USAGE);
    return 2;
  }
  if (args.help) {
    out(USAGE);
    return 0;
  }

  const locales = args.locale ? [args.locale] : LOCALE_FILES;
  let total = 0;
  for (const locale of locales) {
    const { problems, report } = verifyLocale(args.root, locale, args.baseline);
    total += problems;
    out(`── ${locale} ──`);
    out(`   keys=${report.keyCount} missing=${report.missing.length} extra=${report.extra.length}`);
    if (report.missing.length) out(`     missing: ${report.missing.join(', ')}`);
    if (report.extra.length) out(`     extra: ${report.extra.join(', ')}`);
    out(`   placeholder mismatches=${report.placeholderMismatches.length}`);
    for (const m of report.placeholderMismatches) {
      out(`     ${m.key}: en=[${m.en}] loc=[${m.loc}]`);
    }
    out(`   byte-identical to English=${report.identicalCount} (new vs ${args.baseline}: ${report.newlyIdentical.length})`);
    for (const k of report.newlyIdentical) out(`     NEW: ${k}`);
    out(`   entry integrity: ${report.corrupt.length} broken`);
    for (const c of report.corrupt) out(`     ${c.file}:${c.line} ${c.key}`);
  }
  out(total === 0 ? '\n✔ all checks clean' : `\n✘ ${total} problem(s)`);
  return total === 0 ? 0 : 1;
}

if (process.argv[1] && process.argv[1].endsWith('i18n-sync-verify.mjs')) {
  process.exitCode = runCli();
}
