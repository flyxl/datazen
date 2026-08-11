#!/usr/bin/env node
/**
 * e2e-tauri-build.mjs — `tauri build --debug` with webdriver + plugin features.
 *
 * Must run inside `with-plugin-inject` (or after resolve-drivers) so
 * `.plugin-features.json` exists. Mirrors `ci-tauri-build.mjs` but always
 * enables the webdriver feature for WDIO.
 *
 * Usage:
 *   node scripts/with-plugin-inject.mjs --drivers=basic -- node scripts/e2e-tauri-build.mjs
 */

import { readFileSync, existsSync } from 'fs';
import { resolve, dirname } from 'path';
import { fileURLToPath } from 'url';
import { spawnTauri } from './ci-tauri-build.mjs';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, '..');

const featuresPath = resolve(ROOT, '.plugin-features.json');
if (!existsSync(featuresPath)) {
  console.error('[e2e-tauri-build] missing .plugin-features.json — run resolve-drivers / with-plugin-inject first');
  process.exit(1);
}

const { features } = JSON.parse(readFileSync(featuresPath, 'utf-8'));
const featureList = ['webdriver', ...(Array.isArray(features) ? features : [])];
const args = ['build', '--debug', '-f', featureList.join(',')];

const result = spawnTauri(args, {
  log: (msg) => console.log(msg.replace('[ci-tauri-build]', '[e2e-tauri-build]')),
});
process.exit(result.status ?? 1);
