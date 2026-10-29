#!/usr/bin/env node
/**
 * Align a locale's terminology with a rule set.
 *
 * ## Why this exists
 *
 * A translation round runs several translators over one locale's keys. Two
 * of them can pick different words for the SAME concept — a "saved view"
 * might come back as 筛选方案 in one file and 视图 in another, or a locale
 * might drift from the term the project's own design doc uses. Both are
 * real defects, but fixing them by hand means touching every affected
 * string, and doing it by hand is exactly where a "replace 方案 everywhere"
 * accident happens.
 *
 * This tool applies an explicit, reviewable rule set instead: each rule names
 * the exact old value and the exact new value for one key. Nothing is
 * rewritten by pattern, so a rule can never hit a key it was not written for.
 *
 * ## The rules file
 *
 * ```json
 * {
 *   "zh-CN": {
 *     "filter.view.savedViews": { "from": "已保存的筛选方案", "to": "已保存的命名视图" },
 *     "filter.view.nameLabel":   { "from": "筛选方案名称",       "to": "命名视图名称" }
 *   }
 * }
 * ```
 *
 * `from` is optional; when present it is asserted before the write, so a
 * rule written against an older wording fails loudly instead of silently
 * overwriting something else. `to` is required.
 *
 * ## Idempotence
 *
 * A rule whose `from` already equals the current value is a no-op, and a
 * rule whose `to` already equals the current value is a no-op too. Running
 * the tool twice changes nothing the second time.
 *
 * ## Usage
 *
 * ```bash
 * node scripts/i18n-sync-harmonize.mjs --rules=rules.json [--locale=zh-CN] [--dry-run]
 * node scripts/i18n-sync-harmonize.mjs --help
 * ```
 */
import { readFileSync, writeFileSync, existsSync, readdirSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { escapeTsValue, parseEntries } from './lib/i18nSync.mjs';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, '..');

const USAGE = `Usage: node scripts/i18n-sync-harmonize.mjs --rules=<rules.json> [options]

Align a locale's terminology with an explicit rule set.

The rules file maps a locale to the keys it should change:

  { "zh-CN": { "<key>": { "from": "<old>", "to": "<new>" } } }

Options:
  --rules=<path>    rules JSON                          (required)
  --locale=<code>  only this locale (default: every locale in the file)
  --root=<path>    repository root                     (default: the repo)
  --dry-run        report what would change, write nothing
  --help           print this help

Each rule names one key and its exact new value; "from", when present, is
asserted before the write so a stale rule fails loudly instead of
overwriting something else.

Idempotent: a rule already satisfied changes nothing.`;

function parseArgs(argv) {
  const args = { root: ROOT, help: false, rules: null, locale: null, dryRun: false };
  for (const raw of argv) {
    let matched = false;
    if (raw === '--help' || raw === '-h') {
      args.help = true;
      continue;
    }
    if (raw === '--dry-run') {
      args.dryRun = true;
      continue;
    }
    for (const [flag, key] of [
      ['--rules=', 'rules'],
      ['--locale=', 'locale'],
      ['--root=', 'root'],
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
 * Apply the rules for one locale to its domain packs.
 *
 * @param {string} root
 * @param {string} locale
 * @param {Record<string, { from?: string, to: string }>} rules
 * @param {{ dryRun?: boolean }} options
 */
export function harmonizeLocale(root, locale, rules, { dryRun = false } = {}) {
  const localeDir = resolve(root, 'packages/frontend/src/locales', locale);
  const changed = [];
  const problems = [];
  const noop = [];
  if (!existsSync(localeDir)) return { changed, problems, noop };

  // Which pack holds which key.
  const files = readdirSync(localeDir).filter((f) => f.endsWith('.ts')).sort();
  /** key → { file, value } */
  const located = new Map();
  for (const f of files) {
    const source = readFileSync(resolve(localeDir, f), 'utf-8');
    for (const e of parseEntries(source)) {
      if (!located.has(e.key)) located.set(e.key, { file: f, value: e.value });
    }
  }

  // Group the changes per file so each file is rewritten once.
  const perFile = new Map();
  for (const [key, rule] of Object.entries(rules)) {
    const found = located.get(key);
    if (!found) {
      problems.push(`key not present in ${locale}: ${key}`);
      continue;
    }
    // Already at the target value: nothing to do, whatever `from` says.
    // This is what makes a second run a no-op.
    if (found.value === rule.to) {
      noop.push(key);
      continue;
    }
    if (rule.from !== undefined && found.value !== rule.from) {
      problems.push(
        `${locale}/${key}: expected ${JSON.stringify(rule.from)}, found ${JSON.stringify(found.value)}`,
      );
      continue;
    }
    if (!perFile.has(found.file)) perFile.set(found.file, []);
    perFile.get(found.file).push({ key, value: rule.to, from: found.value });
    changed.push(key);
  }

  if (!dryRun) {
    for (const [file, edits] of perFile) {
      const path = resolve(localeDir, file);
      const lines = readFileSync(path, 'utf-8').split('\n');
      // Rewrite from the bottom up so the earlier line numbers stay valid.
      const byKey = new Map(parseEntries(readFileSync(path, 'utf-8')).map((e) => [e.key, e]));
      const ordered = [...edits].sort(
        (a, b) => byKey.get(b.key).line - byKey.get(a.key).line,
      );
      for (const edit of ordered) {
        const entry = byKey.get(edit.key);
        const rendered = `  '${edit.key}': '${escapeTsValue(edit.value)}',`;
        if (entry.endLine === entry.line) {
          lines[entry.line] = rendered;
        } else {
          // The value was wrapped onto the next line; keep that shape.
          lines[entry.line] = `  '${edit.key}':`;
          lines[entry.endLine] = `    '${escapeTsValue(edit.value)}',`;
        }
      }
      writeFileSync(path, lines.join('\n'));
    }
  }

  return { changed, problems, noop };
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
  if (args.help || !args.rules) {
    out(USAGE);
    return args.help ? 0 : 2;
  }

  const rules = JSON.parse(readFileSync(resolve(args.root, args.rules), 'utf-8'));
  const locales = args.locale ? [args.locale] : Object.keys(rules);
  let problems = 0;
  let changedTotal = 0;
  for (const locale of locales) {
    const result = harmonizeLocale(args.root, locale, rules[locale] ?? {}, {
      dryRun: args.dryRun,
    });
    changedTotal += result.changed.length;
    problems += result.problems.length;
    out(`${locale}: changed ${result.changed.length}, already satisfied ${result.noop.length}`);
    for (const k of result.changed) out(`  ${k}`);
    for (const p of result.problems) err(`  ${p}`);
  }
  if (args.dryRun) out('(dry run — nothing written)');
  out(changedTotal === 0 ? '\n✔ nothing to change' : `\n${changedTotal} string(s) changed`);
  return problems === 0 ? 0 : 1;
}

if (process.argv[1] && process.argv[1].endsWith('i18n-sync-harmonize.mjs')) {
  process.exitCode = runCli();
}
