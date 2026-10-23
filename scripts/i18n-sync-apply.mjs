#!/usr/bin/env node
/**
 * Insert a batch of translated locale keys into the locale domain packs.
 *
 * This is the write half of the i18n sync toolchain. The read half is
 * `scripts/i18n-sync-check.mjs` (the release blocker) and the audit half is
 * `scripts/i18n-sync-verify.mjs`.
 *
 * ## What it does
 *
 * Given a batch of `{ "<domain>.ts": { "<key>": "<value>" } }`, it inserts
 * every key into `src/locales/<locale>/<domain>.ts`, positioned the way the
 * English side orders it: a new key lands **right after the closest English
 * predecessor that already exists in the target file**, so each key sits next
 * to its family instead of piling up at the end of the file.
 *
 * ## Idempotence
 *
 * Running it twice is a no-op the second time: a key that is already present
 * with the SAME value is skipped, and a key that is already present with a
 * DIFFERENT value is an error unless `--force` is given. That matters because
 * the batches are re-run after every rebase of the English side, and a tool
 * that duplicated or fought with its own output would be useless.
 *
 * ## The anchor bug this tool exists because of
 *
 * Prettier wraps a long value onto the NEXT line:
 *
 * ```ts
 *   'pagination.estimateTooltip':
 *     'Estimated row count — the real number may differ. Use Count for the exact value.',
 * ```
 *
 * An earlier version of this tool anchored on the line the key is *declared*
 * on, which put the new keys **between the key and its value** and left the
 * file syntactically broken. The anchor is now the line the entry *ends* on.
 *
 * ## Usage
 *
 * ```bash
 * node scripts/i18n-sync-apply.mjs --locale=zh-CN --file=batch.json [--dry-run] [--force]
 * node scripts/i18n-sync-apply.mjs --help
 * ```
 *
 * The batch file is a JSON object mapping a domain pack file name to its
 * translations:
 *
 * ```json
 * {
 *   "query.ts": { "filter.view.savedViews": "已保存的命名视图" },
 *   "settings.ts": { "settings.rowCountMode": "行数统计模式" }
 * }
 * ```
 */
import { readFileSync, writeFileSync, existsSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseEntries, renderEntry } from './lib/i18nSync.mjs';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, '..');

const USAGE = `Usage: node scripts/i18n-sync-apply.mjs --locale=<locale> --file=<batch.json> [options]

Insert translated keys into src/locales/<locale>/<domain>.ts, positioned the
way the English side orders them (right after the closest English predecessor
that already exists in the target file).

Arguments:
  --locale=<locale>   target locale, e.g. zh-CN          (required)
  --file=<path>       batch JSON: { "<domain>.ts": { "<key>": "<value>" } }
                                                          (required)
  --root=<path>       repository root                     (default: the repo)
  --dry-run           report what would change, write nothing
  --force             overwrite a key that already exists with a different value
  --help              print this help

The batch is keyed by domain pack file name because that is the level the
English source of truth is organised on (src/locales/en/<domain>.ts). Keys
must exist in the English pack; a key the English side does not have is an
error, not a silent write.

Idempotent: a key already present with the same value is skipped; one present
with a different value needs --force.`;

function parseArgs(argv) {
  const args = { root: ROOT, help: false, dryRun: false, force: false };
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
    if (raw === '--force') {
      args.force = true;
      continue;
    }
    for (const [flag, key] of [
      ['--locale=', 'locale'],
      ['--file=', 'file'],
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
 * Plan the insertion of one domain pack.
 *
 * @param {string} enSource    English pack source
 * @param {string} localeSource target pack source
 * @param {Record<string, string>} entries key → value
 * @param {boolean} force
 * @returns {{ lines: string[], inserted: string[], skipped: string[], conflicts: string[] }}
 */
export function planInsertion(enSource, localeSource, entries, force) {
  const enEntries = parseEntries(enSource);
  const enOrder = enEntries.map((e) => e.key);
  const enIndex = new Map(enOrder.map((k, i) => [k, i]));
  const localeEntries = parseEntries(localeSource);
  const localeSet = new Set(localeEntries.map((e) => e.key));

  // The end line of every existing entry: the anchor must be the line an
  // entry ENDS on, not the line its key is declared on (see the file header).
  const endLineOf = new Map();
  for (const e of localeEntries) if (!endLineOf.has(e.key)) endLineOf.set(e.key, e.endLine);

  const lines = localeSource.split('\n');
  const inserted = [];
  const skipped = [];
  const conflicts = [];
  const overwritten = [];
  /** anchor key → entries to insert after it */
  const pending = new Map();
  /** keys to overwrite in place: key → new value */
  const replaceInPlace = new Map();

  for (const [key, value] of Object.entries(entries)) {
    if (!enIndex.has(key)) {
      conflicts.push(`key not present in the English pack: ${key}`);
      continue;
    }
    if (localeSet.has(key)) {
      const existing = localeEntries.find((e) => e.key === key).value;
      if (existing === value) {
        skipped.push(key);
      } else if (force) {
        // --force: rewrite the value where the key already stands.
        replaceInPlace.set(key, value);
        overwritten.push(key);
      } else {
        conflicts.push(
          `key already present with a different value: ${key} (existing: ${JSON.stringify(existing)})`,
        );
      }
      continue;
    }
    let anchor = null;
    for (let i = enIndex.get(key) - 1; i >= 0; i--) {
      if (localeSet.has(enOrder[i])) {
        anchor = enOrder[i];
        break;
      }
    }
    if (!anchor) {
      conflicts.push(`no anchor for ${key}: no preceding English key exists in the target file`);
      continue;
    }
    if (!pending.has(anchor)) pending.set(anchor, []);
    pending.get(anchor).push({ key, value, order: enIndex.get(key) });
  }

  // Splice from the bottom up so the earlier line numbers stay valid.
  const workLines = lines.slice();
  const anchors = [...pending.keys()].sort((a, b) => endLineOf.get(b) - endLineOf.get(a));
  for (const anchor of anchors) {
    const at = endLineOf.get(anchor);
    const block = pending.get(anchor).sort((a, b) => a.order - b.order);
    const text = block.map((b) => renderEntry(b.key, b.value)).join('\n');
    workLines.splice(at + 1, 0, text);
    for (const b of block) inserted.push(b.key);
  }

  // --force rewrites: replace the value where the key already stands. Done
  // after the splices, and bottom-up, so the line numbers stay valid.
  if (replaceInPlace.size > 0) {
    const byKey = new Map(localeEntries.map((e) => [e.key, e]));
    const ordered = [...replaceInPlace.keys()].sort(
      (a, b) => byKey.get(b).line - byKey.get(a).line,
    );
    for (const key of ordered) {
      const entry = byKey.get(key);
      const rendered = renderEntry(key, replaceInPlace.get(key));
      const renderedLines = rendered.split('\n');
      if (entry.endLine === entry.line) {
        workLines[entry.line] = renderedLines[0];
      } else {
        // The old value was wrapped onto the next line; keep that shape.
        workLines[entry.line] = renderedLines[0];
        workLines[entry.endLine] = renderedLines[1] ?? renderedLines[0];
      }
    }
  }

  return { lines: workLines, inserted, skipped, overwritten, conflicts };
}

/**
 * Apply a whole batch.
 *
 * @param {string} root
 * @param {string} locale
 * @param {Record<string, Record<string, string>>} batch
 * @param {{ dryRun?: boolean, force?: boolean }} options
 */
export function applyBatch(root, locale, batch, { dryRun = false, force = false } = {}) {
  const localeDir = resolve(root, 'src/locales', locale);
  const enDir = resolve(root, 'src/locales/en');
  const result = { inserted: [], skipped: [], overwritten: [], conflicts: [], written: [] };

  for (const [domainFile, entries] of Object.entries(batch)) {
    const enFile = resolve(enDir, domainFile);
    const localeFile = resolve(localeDir, domainFile);
    if (!existsSync(enFile)) {
      result.conflicts.push(`no English domain pack: ${domainFile}`);
      continue;
    }
    if (!existsSync(localeFile)) {
      result.conflicts.push(`no ${locale} domain pack: ${domainFile}`);
      continue;
    }
    const plan = planInsertion(
      readFileSync(enFile, 'utf-8'),
      readFileSync(localeFile, 'utf-8'),
      entries,
      force,
    );
    result.inserted.push(...plan.inserted);
    result.skipped.push(...plan.skipped);
    result.overwritten.push(...plan.overwritten);
    result.conflicts.push(...plan.conflicts.map((c) => `${domainFile}: ${c}`));
    if ((plan.inserted.length > 0 || plan.overwritten.length > 0) && !dryRun) {
      writeFileSync(localeFile, plan.lines.join('\n'));
      result.written.push(domainFile);
    }
  }
  return result;
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
  if (args.help || !args.locale || !args.file) {
    out(USAGE);
    return args.help ? 0 : 2;
  }

  const batch = JSON.parse(readFileSync(resolve(args.root, args.file), 'utf-8'));
  const result = applyBatch(args.root, args.locale, batch, {
    dryRun: args.dryRun,
    force: args.force,
  });

  out(`${args.locale}: inserted ${result.inserted.length}, skipped ${result.skipped.length}`);
  if (result.written.length) out(`  wrote: ${result.written.join(', ')}`);
  if (args.dryRun && result.inserted.length) out('  (dry run — nothing written)');
  if (result.skipped.length) {
    out(`  already present, same value (no-op): ${result.skipped.join(', ')}`);
  }
  if (result.conflicts.length) {
    err(`conflicts (${result.conflicts.length}):`);
    for (const c of result.conflicts) err(`  ${c}`);
    return 1;
  }
  return 0;
}

if (process.argv[1] && process.argv[1].endsWith('i18n-sync-apply.mjs')) {
  process.exitCode = runCli();
}
