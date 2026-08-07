#!/usr/bin/env node
/**
 * Generate src-tauri/resources/menu-labels.json from frontend locale files.
 *
 * Single source of truth: src/locales/*.ts
 * Rust native menus (and popup context menus) consume the generated JSON
 * via include_str! — they cannot import TypeScript at runtime.
 *
 * Usage:
 *   node scripts/generate-menu-labels.mjs
 *
 * Wired into `pnpm build`, `pnpm build:with-plugins`, `pnpm tauri:build`, and `pnpm tauri:dev`.
 */

import { readFileSync, writeFileSync } from 'fs';
import { dirname, resolve } from 'path';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, '..');
const OUT = resolve(ROOT, 'src-tauri/resources/menu-labels.json');

/**
 * Rust menu lookup key → frontend TranslationKey.
 * Keep Rust keys stable (menu item ids / historical JSON keys);
 * only the string values come from locales.
 */
const RUST_TO_LOCALE = {
  'app-name': 'menu.appName',
  about: 'menu.about',
  services: 'menu.services',
  hide: 'menu.hide',
  'hide-others': 'menu.hideOthers',
  'show-all': 'menu.showAll',
  quit: 'menu.quit',
  file: 'menu.file',
  edit: 'menu.edit',
  undo: 'menu.undo',
  redo: 'menu.redo',
  cut: 'menu.cut',
  copy: 'menu.copy',
  paste: 'menu.paste',
  'select-all': 'menu.selectAll',
  view: 'menu.view',
  theme: 'menu.theme',
  tools: 'menu.tools',
  window: 'menu.window',
  minimize: 'menu.minimize',
  zoom: 'menu.zoom',
  'bring-all-to-front': 'menu.bringAllToFront',
  'close-window': 'menu.closeWindow',
  fullscreen: 'menu.fullscreen',
  help: 'menu.help',
  documentation: 'menu.documentation',
  'report-issue': 'menu.reportIssue',
  'theme-light': 'menu.themeLight',
  'theme-dark': 'menu.themeDark',
  'theme-system': 'menu.themeSystem',
  'open-settings': 'menu.settings',
  'new-connection': 'menu.newConnection',
  'data-sync': 'menu.dataSync',
  backup: 'menu.backup',
  restore: 'menu.restore',
  'export-config': 'menu.exportConfig',
  'import-config': 'menu.importConfig',
  'export-connections': 'menu.exportConnections',
  'import-connections': 'menu.importConnections',
  'view-logs': 'menu.viewLogs',
  'ctx-add-favorite': 'menu.ctxAddFavorite',
};

const LOCALES = [
  { code: 'zh-CN', file: 'src/locales/zh-CN.ts' },
  { code: 'en', file: 'src/locales/en.ts' },
  { code: 'zh-TW', file: 'src/locales/zh-TW.ts' },
  { code: 'es', file: 'src/locales/es.ts' },
  { code: 'fr', file: 'src/locales/fr.ts' },
  { code: 'de', file: 'src/locales/de.ts' },
  { code: 'ja', file: 'src/locales/ja.ts' },
  { code: 'pt-BR', file: 'src/locales/pt-BR.ts' },
  { code: 'ru', file: 'src/locales/ru.ts' },
  { code: 'ko', file: 'src/locales/ko.ts' },
];

function parseLocaleTs(source) {
  const map = new Map();
  // Matches: 'key': 'value'  (value may contain …, {param}, escaped \')
  const re = /'((?:\\'|[^'])*)':\s*'((?:\\'|[^'])*)'/g;
  let m;
  while ((m = re.exec(source)) !== null) {
    const key = m[1].replace(/\\'/g, "'");
    const value = m[2].replace(/\\'/g, "'");
    map.set(key, value);
  }
  return map;
}

function buildLang(localeMap, code, fallbackMap) {
  const out = {};
  const missing = [];
  for (const [rustKey, localeKey] of Object.entries(RUST_TO_LOCALE)) {
    const value = localeMap.get(localeKey) ?? fallbackMap?.get(localeKey);
    if (value === undefined) {
      missing.push(localeKey);
      continue;
    }
    out[rustKey] = value;
  }
  if (missing.length > 0) {
    throw new Error(
      `[generate-menu-labels] ${code} missing keys (no en fallback):\n  - ${missing.join('\n  - ')}`,
    );
  }
  return out;
}

const enPath = resolve(ROOT, 'src/locales/en.ts');
const enFallback = parseLocaleTs(readFileSync(enPath, 'utf-8'));

const result = {};
for (const { code, file } of LOCALES) {
  const path = resolve(ROOT, file);
  const source = readFileSync(path, 'utf-8');
  const localeMap = parseLocaleTs(source);
  result[code] = buildLang(localeMap, code, enFallback);
}

const json = `${JSON.stringify(result, null, 2)}\n`;
writeFileSync(OUT, json, 'utf-8');
console.log(
  `[generate-menu-labels] wrote ${OUT} (${Object.keys(RUST_TO_LOCALE).length} keys × ${LOCALES.length} locales)`,
);
