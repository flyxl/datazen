#!/usr/bin/env node
/**
 * Build src/locales/{locale}.ts from en.ts keys + locale JSON overrides.
 * Usage: node scripts/build-locale-from-json.mjs de scripts/locale-data/de.json
 */
import { readFileSync, writeFileSync } from 'fs';
import { dirname, resolve } from 'path';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, '..');

function parseLocaleTs(source) {
  const map = new Map();
  const re = /'((?:\\'|[^'])*)':\s*'((?:\\'|[^'])*)'/g;
  let m;
  while ((m = re.exec(source)) !== null) {
    const key = m[1].replace(/\\'/g, "'");
    const value = m[2].replace(/\\'/g, "'").replace(/\\n/g, '\n');
    map.set(key, value);
  }
  return map;
}

function escapeTs(value) {
  return value.replace(/\\/g, '\\\\').replace(/'/g, "\\'").replace(/\n/g, '\\n');
}

const locale = process.argv[2];
const jsonPath = resolve(ROOT, process.argv[3]);
if (!locale || !jsonPath) {
  console.error('Usage: node scripts/build-locale-from-json.mjs <locale> <json-path>');
  process.exit(1);
}

const enMap = parseLocaleTs(readFileSync(resolve(ROOT, 'src/locales/en.ts'), 'utf-8'));
const overrides = JSON.parse(readFileSync(jsonPath, 'utf-8'));

const missing = [];
const extra = [];
for (const key of Object.keys(overrides)) {
  if (!enMap.has(key)) extra.push(key);
}
for (const key of enMap.keys()) {
  if (!(key in overrides)) missing.push(key);
}
if (extra.length > 0) {
  console.warn(`[build-locale] ${locale}: extra keys (${extra.length}): ${extra.slice(0, 5).join(', ')}...`);
}
if (missing.length > 0) {
  console.error(`[build-locale] ${locale}: missing keys (${missing.length}):\n  - ${missing.join('\n  - ')}`);
  process.exit(1);
}

const lines = [];
for (const [key, enVal] of enMap) {
  const val = overrides[key] ?? enVal;
  lines.push(`  '${key}': '${escapeTs(val)}',`);
}

const out = `const translations = {\n${lines.join('\n')}\n} as const;\n\nexport type TranslationKey = keyof typeof translations;\nexport default translations;\n`;
const outPath = resolve(ROOT, `src/locales/${locale}.ts`);
writeFileSync(outPath, out, 'utf-8');
console.log(`[build-locale] wrote ${outPath} (${enMap.size} keys)`);
