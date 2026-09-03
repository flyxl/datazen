#!/usr/bin/env node
/**
 * Split legacy locale files into the same domain packs used by en/zh-CN.
 * Missing translations intentionally fall back to the English value.
 *
 * Usage:
 *   node scripts/split-locale-packs.mjs [locale ...]
 */

import { mkdirSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const LOCALES = process.argv.slice(2);
const TARGET_LOCALES = LOCALES.length > 0
  ? LOCALES
  : ['de', 'es', 'fr', 'ja', 'ko', 'pt-BR', 'ru', 'zh-TW'];
const DOMAIN_DIR = resolve(ROOT, 'src/locales/en');
const DOMAINS = readdirSync(DOMAIN_DIR)
  .filter((name) => name.endsWith('.ts') && !['eager.ts', 'index.ts'].includes(name))
  .map((name) => name.slice(0, -3))
  .sort();

function parseLocaleTs(source) {
  const map = new Map();
  const re = /'((?:\\.|[^'\\])*)':\s*(?:"((?:\\.|[^"\\])*)"|'((?:\\.|[^'\\])*)')/g;
  let match;
  while ((match = re.exec(source)) !== null) {
    const unescape = (value) => value.replace(/\\(['"\\])/g, '$1').replace(/\\n/g, '\n');
    map.set(
      unescape(match[1]),
      unescape(match[2] ?? match[3]),
    );
  }
  return map;
}

function escapeTs(value) {
  return value.replace(/\\/g, '\\\\').replace(/'/g, "\\'").replace(/\n/g, '\\n');
}

function readPack(domain, locale = 'en') {
  return parseLocaleTs(
    readFileSync(resolve(ROOT, 'src/locales', locale, `${domain}.ts`), 'utf-8'),
  );
}

function writePack(locale, domain, translations) {
  const lines = [...translations].map(
    ([key, value]) => `  '${key}': '${escapeTs(value)}',`,
  );
  const content = [
    `/** Auto-split domain: ${domain} (${locale}) */`,
    'const pack = {',
    ...lines,
    '} as const;',
    'export default pack;',
    '',
  ].join('\n');
  writeFileSync(resolve(ROOT, 'src/locales', locale, `${domain}.ts`), content);
}

function writeIndex(locale, eagerDomains) {
  const imports = DOMAINS.map((domain) => `import ${domain} from './${domain}';`);
  const spreads = DOMAINS.map((domain) => `  ...${domain},`);
  writeFileSync(
    resolve(ROOT, 'src/locales', locale, 'index.ts'),
    [
      `/** Full ${locale} dictionary (all domains merged). */`,
      ...imports,
      '',
      'const locale = {',
      ...spreads,
      '} as const;',
      '',
      'export default locale;',
      'export type TranslationKey = keyof typeof locale;',
      '',
    ].join('\n'),
  );

  const eagerImports = eagerDomains.map((domain) => `import ${domain} from './${domain}';`);
  const eagerSpreads = eagerDomains.map((domain) => `  ...${domain},`);
  writeFileSync(
    resolve(ROOT, 'src/locales', locale, 'eager.ts'),
    [
      `/** Eager domain packs for ${locale} (always in main chunk). */`,
      ...eagerImports,
      '',
      'const eager = {',
      ...eagerSpreads,
      '} as const;',
      '',
      'export default eager;',
      '',
    ].join('\n'),
  );
}

const eagerDomains = ['core', 'connection', 'schema', 'query', 'settings', 'chart', 'backup', 'ai'];
const english = new Map();
for (const domain of DOMAINS) {
  for (const [key, value] of readPack(domain)) english.set(key, value);
}

for (const locale of TARGET_LOCALES) {
  const legacyPath = resolve(ROOT, 'src/locales', `${locale}.ts`);
  let legacySource = readFileSync(legacyPath, 'utf-8');
  let legacy = parseLocaleTs(legacySource);
  if (legacy.size === 0) {
    try {
      legacySource = execFileSync(
        'git',
        ['show', `HEAD:src/locales/${locale}.ts`],
        { cwd: ROOT, encoding: 'utf-8' },
      );
      legacy = parseLocaleTs(legacySource);
    } catch {
      // Keep the empty map; all keys will use the English fallback below.
    }
  }
  const localeDir = resolve(ROOT, 'src/locales', locale);
  mkdirSync(localeDir, { recursive: true });
  let missing = 0;

  for (const domain of DOMAINS) {
    const translations = new Map();
    for (const [key, englishValue] of readPack(domain)) {
      if (!legacy.has(key)) missing += 1;
      translations.set(key, legacy.get(key) ?? englishValue);
    }
    writePack(locale, domain, translations);
  }
  writeIndex(locale, eagerDomains);
  writeFileSync(
    resolve(ROOT, 'src/locales', `${locale}.ts`),
    `/** Backward-compatible entry: full ${locale} dictionary. */\nexport { default, type TranslationKey } from './${locale}/index';\n`,
  );
  console.log(`[split-locale-packs] ${locale}: ${english.size} keys, ${missing} English fallback(s)`);
}
