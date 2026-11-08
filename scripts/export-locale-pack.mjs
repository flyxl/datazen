#!/usr/bin/env node
/**
 * Export a source-only locale as an installable language pack ZIP.
 *
 * The repository keeps 10 locale dictionaries but bundles only en / zh-CN.
 * This script turns one of the source-only locales (de, es, fr, ja, ko,
 * pt-BR, ru, zh-TW) into the external pack format documented in
 * docs/development/language-pack-install-design.md:
 *
 *   datazen-locale-<code>.zip
 *     manifest.json        { packVersion, code, label, version }
 *     core.json … mcp.json  flat key → message maps, one file per domain
 *
 * Usage:
 *   node scripts/export-locale-pack.mjs <code> [outDir]
 *
 * The ZIP is written with the store method (no compression): pack JSON is
 * already compact and determinism beats size for a source-generated artifact.
 */

import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, '..');
const LOCALES_DIR = resolve(ROOT, 'packages/frontend/src/locales');

const ENDONYMS = {
  en: 'English',
  'zh-CN': '简体中文',
  'zh-TW': '繁體中文',
  es: 'Español',
  fr: 'Français',
  de: 'Deutsch',
  ja: '日本語',
  'pt-BR': 'Português (Brasil)',
  ru: 'Русский',
  ko: '한국어',
};

const DOMAINS = [
  'core',
  'connection',
  'schema',
  'query',
  'settings',
  'chart',
  'backup',
  'ai',
  'onboarding',
  'sync',
  'workflows',
  'dashboard',
  'mcp',
];

/** Parse a domain pack's `key: 'value'` pairs (same regex family as
 * i18n-sync-check.mjs — no TS parser needed for these flat dictionaries). */
function parseDomainPack(source) {
  const entries = {};
  const re = /^\s*'((?:[^'\\]|\\.)+)':\s*(?:'((?:[^'\\]|\\.)*)'|"((?:[^"\\]|\\.)*)")/gm;
  let match;
  while ((match = re.exec(source)) !== null) {
    const key = match[1].replace(/\\'/g, "'");
    const raw = match[2] ?? match[3] ?? '';
    entries[key] = raw.replace(/\\'/g, "'").replace(/\\n/g, '\n').replace(/\\\\/g, '\\');
  }
  return entries;
}

// ── Minimal ZIP writer (store method, UTF-8 names) ───────────────────────────

const CRC_TABLE = (() => {
  const table = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) {
      c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    }
    table[n] = c >>> 0;
  }
  return table;
})();

function crc32(bytes) {
  let crc = 0xffffffff;
  for (const byte of bytes) {
    crc = CRC_TABLE[(crc ^ byte) & 0xff] ^ (crc >>> 8);
  }
  return (crc ^ 0xffffffff) >>> 0;
}

/** Fixed DOS timestamp (2020-01-01 00:00:00) — deterministic archives. */
const DOS_TIME = 0;
const DOS_DATE = ((2020 - 1980) << 9) | (1 << 5) | 1;

function buildZip(entries) {
  /** [name, Uint8Array] → stored-method zip bytes. */
  const encoder = new TextEncoder();
  const locals = [];
  const centrals = [];
  let offset = 0;
  for (const [name, content] of entries) {
    const nameBytes = encoder.encode(name);
    const crc = crc32(content);
    const local = new Uint8Array(30 + nameBytes.length);
    const localView = new DataView(local.buffer);
    localView.setUint32(0, 0x04034b50, true);
    localView.setUint16(4, 20, true); // version needed
    localView.setUint16(6, 0x0800, true); // UTF-8 names
    localView.setUint16(8, 0, true); // method: store
    localView.setUint16(10, DOS_TIME, true);
    localView.setUint16(12, DOS_DATE, true);
    localView.setUint32(14, crc, true);
    localView.setUint32(18, content.length, true);
    localView.setUint32(22, content.length, true);
    localView.setUint16(26, nameBytes.length, true);
    localView.setUint16(28, 0, true);
    local.set(nameBytes, 30);
    locals.push(local, content);

    const central = new Uint8Array(46 + nameBytes.length);
    const centralView = new DataView(central.buffer);
    centralView.setUint32(0, 0x02014b50, true);
    centralView.setUint16(4, 20, true);
    centralView.setUint16(6, 20, true);
    centralView.setUint16(8, 0x0800, true);
    centralView.setUint16(10, 0, true);
    centralView.setUint16(12, DOS_TIME, true);
    centralView.setUint16(14, DOS_DATE, true);
    centralView.setUint32(16, crc, true);
    centralView.setUint32(20, content.length, true);
    centralView.setUint32(24, content.length, true);
    centralView.setUint16(28, nameBytes.length, true);
    centralView.setUint32(42, offset, true);
    central.set(nameBytes, 46);
    centrals.push(central);

    offset += local.length + content.length;
  }
  const centralSize = centrals.reduce((sum, part) => sum + part.length, 0);
  const eocd = new Uint8Array(22);
  const eocdView = new DataView(eocd.buffer);
  eocdView.setUint32(0, 0x06054b50, true);
  eocdView.setUint16(8, centrals.length, true);
  eocdView.setUint16(10, centrals.length, true);
  eocdView.setUint32(12, centralSize, true);
  eocdView.setUint32(16, offset, true);
  return Buffer.concat([...locals, ...centrals, eocd]);
}

// ── CLI ─────────────────────────────────────────────────────────────────────

const [, , codeArg, outDirArg] = process.argv;
if (!codeArg) {
  console.error('usage: node scripts/export-locale-pack.mjs <code> [outDir]');
  console.error(`  codes: ${Object.keys(ENDONYMS).filter((c) => c !== 'en' && c !== 'zh-CN').join(' ')}`);
  process.exit(1);
}
const code = codeArg;
if (code === 'en' || code === 'zh-CN') {
  console.error(`[export-locale-pack] ${code} is bundled — nothing to export`);
  process.exit(1);
}
if (!ENDONYMS[code]) {
  console.error(`[export-locale-pack] unknown locale code: ${code}`);
  process.exit(1);
}

const appVersion = JSON.parse(readFileSync(join(ROOT, 'package.json'), 'utf-8')).version;

const manifest = {
  packVersion: 1,
  code,
  label: ENDONYMS[code],
  version: appVersion,
};

const entries = [
  ['manifest.json', new TextEncoder().encode(JSON.stringify(manifest, null, 2) + '\n')],
];
let messageCount = 0;
const domainStats = [];

for (const domain of DOMAINS) {
  const domainFile = join(LOCALES_DIR, code, `${domain}.ts`);
  if (!existsSync(domainFile)) {
    console.warn(`[export-locale-pack] ${domain}.ts missing — domain skipped`);
    continue;
  }
  const pack = parseDomainPack(readFileSync(domainFile, 'utf-8'));
  const count = Object.keys(pack).length;
  if (count === 0) {
    continue;
  }
  messageCount += count;
  domainStats.push(`${domain}:${count}`);
  entries.push([`${domain}.json`, new TextEncoder().encode(JSON.stringify(pack, null, 2) + '\n')]);
}

if (messageCount === 0) {
  console.error(`[export-locale-pack] no messages found for ${code}`);
  process.exit(1);
}

const outDir = outDirArg ? resolve(ROOT, outDirArg) : ROOT;
mkdirSync(outDir, { recursive: true });
const zipPath = join(outDir, `datazen-locale-${code}.zip`);
const zipBytes = buildZip(entries);
writeFileSync(zipPath, zipBytes);

console.log(
  `[export-locale-pack] wrote ${zipPath} (${(zipBytes.length / 1024).toFixed(1)} KB, ` +
    `${messageCount} messages, ${domainStats.length} domains: ${domainStats.join(' ')})`,
);
