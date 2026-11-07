#!/usr/bin/env node
/**
 * Audit a translation batch BEFORE it is applied to the locale packs.
 *
 * The batch is the unit an i18n sync round produces: one JSON file holding
 * every key the round translates, grouped by the English domain pack the key
 * belongs to. This script checks that batch on its own — without touching the
 * repository — so a batch that would break the locale packs is caught while
 * it is still just a file.
 *
 * ## What it checks
 *
 * 1. **Key parity with the English side** — every key in the batch exists in
 *    the English pack, and every English key the batch claims to cover is
 *    actually there. A key the English side does not have is an error: the
 *    English packs are the single source of truth.
 * 2. **Placeholder parity** — the SET of `{name}` placeholder names equals
 *    the English set. `${1:tab}` tab stops and JSON braces are not
 *    placeholders and are excluded.
 * 3. **Not byte-identical to English** — the gate's stale rule is a byte
 *    comparison, so a value equal to the English is reported. A value that
 *    is *deliberately* the same (a technical token that stays untranslated)
 *    is still reported: the decision to keep it is a human one, and this
 *    script only points at it.
 * 4. **Literal safety** — no raw ASCII apostrophe or backslash that would
 *    need escaping inside a single-quoted TypeScript literal, and no control
 *    characters.
 *
 * ## Usage
 *
 * ```bash
 * node scripts/i18n-sync-verify-batch.mjs --file=batch.json [--locale=zh-CN] [--root=<path>]
 * node scripts/i18n-sync-verify-batch.mjs --help
 * ```
 */
import { readFileSync, existsSync, readdirSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { extractPlaceholders, parsePack } from './lib/i18nSync.mjs';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, '..');

const USAGE = `Usage: node scripts/i18n-sync-verify-batch.mjs --file=<batch.json> [options]

Audit a translation batch before it is applied to the locale packs. The batch
is a JSON object mapping a domain pack file name to its translations:

  { "query.ts": { "<key>": "<value>" } }

Options:
  --file=<path>    batch JSON                        (required)
  --locale=<code>  the locale the batch is for (informational)
  --root=<path>    repository root                   (default: the repo)
  --help           print this help

Checks: key parity with the English side · placeholder-name parity ·
byte-identical-to-English (the gate's stale rule) · literal safety.

Exit codes: 0 clean · 1 problems found · 2 bad usage.`;

function parseArgs(argv) {
  const args = { root: ROOT, help: false, file: null, locale: null };
  for (const raw of argv) {
    let matched = false;
    if (raw === '--help' || raw === '-h') {
      args.help = true;
      continue;
    }
    for (const [flag, key] of [
      ['--file=', 'file'],
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
 * Audit one batch.
 *
 * @param {string} root
 * @param {Record<string, Record<string, string>>} batch
 * @returns {{ problems: string[], counts: object }}
 */
export function verifyBatch(root, batch) {
  const problems = [];
  const counts = { keys: 0, domains: Object.keys(batch).length };
  const enDir = resolve(root, 'packages/frontend/src/locales/en');

  for (const [domainFile, entries] of Object.entries(batch)) {
    const enFile = resolve(enDir, domainFile);
    if (!existsSync(enFile)) {
      problems.push(`no English domain pack: ${domainFile}`);
      continue;
    }
    const en = parsePack(readFileSync(enFile, 'utf-8'));
    for (const [key, value] of Object.entries(entries)) {
      counts.keys++;
      if (!(key in en)) {
        problems.push(`${domainFile}: ${key} is not an English key`);
        continue;
      }
      const enPlaceholders = extractPlaceholders(en[key]);
      const locPlaceholders = extractPlaceholders(value);
      if (
        enPlaceholders.size !== locPlaceholders.size ||
        [...enPlaceholders].some((p) => !locPlaceholders.has(p))
      ) {
        problems.push(
          `${domainFile}: ${key} placeholder mismatch: en=[${[...enPlaceholders]}] loc=[${[...locPlaceholders]}]`,
        );
      }
      if (value === en[key]) {
        problems.push(
          `${domainFile}: ${key} is byte-identical to English (the gate would call it stale): ${JSON.stringify(value)}`,
        );
      }
      if (value.includes("'")) {
        problems.push(`${domainFile}: ${key} contains a raw apostrophe; it would need escaping`);
      }
      if (value.includes('\\')) {
        problems.push(`${domainFile}: ${key} contains a backslash; it would need escaping`);
      }
      if (/[\r\n\t]/.test(value)) {
        problems.push(`${domainFile}: ${key} contains a control character`);
      }
    }
  }
  return { problems, counts };
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
  if (args.help || !args.file) {
    out(USAGE);
    return args.help ? 0 : 2;
  }

  const batch = JSON.parse(readFileSync(resolve(args.root, args.file), 'utf-8'));
  const { problems, counts } = verifyBatch(args.root, batch);
  out(`batch: ${counts.keys} key(s) across ${counts.domains} domain pack(s)`);
  if (args.locale) out(`locale: ${args.locale}`);
  if (problems.length === 0) {
    out('✔ batch is clean');
    return 0;
  }
  err(`problems (${problems.length}):`);
  for (const p of problems) err(`  ${p}`);
  return 1;
}

if (process.argv[1] && process.argv[1].endsWith('i18n-sync-verify-batch.mjs')) {
  process.exitCode = runCli();
}
