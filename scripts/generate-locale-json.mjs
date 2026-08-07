#!/usr/bin/env node
/**
 * Generate locale JSON from en.ts using Google Translate (batch).
 * Preserves {placeholder} tokens. Usage:
 *   node scripts/generate-locale-json.mjs de
 */
import { readFileSync, writeFileSync, mkdirSync } from 'fs';
import { dirname, resolve } from 'path';
import { fileURLToPath } from 'url';
import translate from '@iamtraction/google-translate';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, '..');
const OUT_DIR = resolve(ROOT, 'scripts/locale-data');

const LOCALE_TO_LANG = {
  de: 'de',
  es: 'es',
  fr: 'fr',
  ja: 'ja',
  ko: 'ko',
  'pt-BR': 'pt',
  ru: 'ru',
};

function parseLocaleTs(source) {
  const entries = [];
  const re = /'((?:\\'|[^'])*)':\s*'((?:\\'|[^'])*)'/g;
  let m;
  while ((m = re.exec(source)) !== null) {
    entries.push([
      m[1].replace(/\\'/g, "'"),
      m[2].replace(/\\'/g, "'").replace(/\\n/g, '\n'),
    ]);
  }
  return entries;
}

const PLACEHOLDER_RE = /\{[a-zA-Z_][a-zA-Z0-9_]*\}/g;

function maskPlaceholders(text) {
  const tokens = [];
  const masked = text.replace(PLACEHOLDER_RE, (match) => {
    const id = `__PH${tokens.length}__`;
    tokens.push({ id, match });
    return id;
  });
  return { masked, tokens };
}

function unmaskPlaceholders(text, tokens) {
  let out = text;
  for (const { id, match } of tokens) {
    out = out.replaceAll(id, match);
  }
  return out;
}

async function translateText(text, to) {
  if (!text.trim()) return text;
  const { masked, tokens } = maskPlaceholders(text);
  const res = await translate(masked, { to, from: 'en' });
  return unmaskPlaceholders(res.text, tokens);
}

async function sleep(ms) {
  return new Promise((r) => setTimeout(r, ms));
}

const locale = process.argv[2];
const lang = LOCALE_TO_LANG[locale];
if (!locale || !lang) {
  console.error('Usage: node scripts/generate-locale-json.mjs <de|es|fr|ja|ko|pt-BR|ru>');
  process.exit(1);
}

mkdirSync(OUT_DIR, { recursive: true });
const enEntries = parseLocaleTs(readFileSync(resolve(ROOT, 'src/locales/en.ts'), 'utf-8'));
const out = {};
const BATCH = 20;
let done = 0;

for (let i = 0; i < enEntries.length; i += BATCH) {
  const batch = enEntries.slice(i, i + BATCH);
  await Promise.all(
    batch.map(async ([key, value]) => {
      // Keep brand names / OK / DataZen untranslated where identical is OK
      if (value === 'OK' || value === 'DataZen' || /^[A-Z0-9\s\-_.:()⌘⌥&]+$/.test(value) && value.length < 4) {
        out[key] = value;
        return;
      }
      try {
        out[key] = await translateText(value, lang);
      } catch (e) {
        console.warn(`[${locale}] failed ${key}: ${e.message}, retry...`);
        await sleep(2000);
        out[key] = await translateText(value, lang);
      }
    }),
  );
  done += batch.length;
  console.log(`[${locale}] ${done}/${enEntries.length}`);
  await sleep(500);
}

const outPath = resolve(OUT_DIR, `${locale}.json`);
writeFileSync(outPath, JSON.stringify(out, null, 2) + '\n');
console.log(`[generate-locale-json] wrote ${outPath}`);
