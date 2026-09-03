#!/usr/bin/env node
/**
 * i18n-sync-check.mjs
 *
 * Compares English locale (en.ts) against all other locale files.
 * Reports missing keys and stale translations (unchanged since last tag).
 *
 * Usage:
 *   node scripts/i18n-sync-check.mjs              # check all locales
 *   node scripts/i18n-sync-check.mjs --from v1.0   # diff from tag v1.0
 *   node scripts/i18n-sync-check.mjs --verbose      # show changed en values
 */

import { execSync } from 'node:child_process';
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = resolve(__dirname, '..');
const localesDir = resolve(root, 'src/locales');

const LOCALE_FILES = ['de', 'es', 'fr', 'ja', 'ko', 'pt-BR', 'ru', 'zh-TW'];

function extractKeys(filePath) {
  const src = readFileSync(filePath, 'utf-8');
  const keys = {};
  const re = /^\s*'([^']+)':\s*(?:'((?:[^'\\]|\\.)*)'|"((?:[^"\\]|\\.)*)")/gm;
  let m;
  while ((m = re.exec(src)) !== null) {
    keys[m[1]] = m[2] ?? m[3] ?? '';
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

function getEnKeysAtRef(ref) {
  try {
    const content = execSync(`git show ${ref}:src/locales/en.ts`, {
      cwd: root,
      encoding: 'utf-8',
      stdio: ['pipe', 'pipe', 'pipe'],
    });
    const keys = {};
    const re = /^\s*'([^']+)':\s*(?:'((?:[^'\\]|\\.)*)'|"((?:[^"\\]|\\.)*)")/gm;
    let m;
    while ((m = re.exec(content)) !== null) {
      keys[m[1]] = m[2] ?? m[3] ?? '';
    }
    return keys;
  } catch {
    return null;
  }
}

function getLatestTag() {
  try {
    return execSync('git describe --tags --abbrev=0', {
      cwd: root,
      encoding: 'utf-8',
    }).trim();
  } catch {
    return null;
  }
}

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
    console.log(`\nComparing en.ts against ${fromRef}: ${changedEnKeys.size} key(s) changed/added\n`);
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
  const stale = changedEnKeys.size > 0
    ? [...changedEnKeys].filter((k) => localeKeySet.has(k) && localeKeys[k] === enKeys[k])
    : [];

  if (missing.length === 0 && extra.length === 0 && stale.length === 0) {
    continue;
  }

  console.log(`[${locale}]`);
  if (missing.length > 0) {
    console.log(`  Missing ${missing.length} key(s): ${missing.slice(0, 10).join(', ')}${missing.length > 10 ? '…' : ''}`);
    totalMissing += missing.length;
  }
  if (extra.length > 0) {
    console.log(`  Extra ${extra.length} key(s): ${extra.slice(0, 5).join(', ')}${extra.length > 5 ? '…' : ''}`);
  }
  if (stale.length > 0) {
    console.log(`  Stale ${stale.length} key(s) (value equals en, may need translation): ${stale.slice(0, 10).join(', ')}${stale.length > 10 ? '…' : ''}`);
    totalStale += stale.length;
  }
}

if (totalMissing === 0 && totalStale === 0) {
  console.log('All locale files are in sync with en.ts.');
} else {
  console.log(`\nSummary: ${totalMissing} missing key(s), ${totalStale} stale translation(s) across ${LOCALE_FILES.length} locales.`);
  process.exitCode = 1;
}
