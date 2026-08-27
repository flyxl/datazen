#!/usr/bin/env node
/**
 * e2e-tauri-build.mjs — `tauri build --debug` with webdriver + plugin features.
 *
 * Must run inside `with-driver-inject` (or after resolve-drivers) so
 * `.driver-features.json` exists. Mirrors `ci-tauri-build.mjs` but always
 * enables the webdriver feature for WDIO.
 *
 * Usage:
 *   node scripts/with-driver-inject.mjs --drivers=basic -- node scripts/e2e-tauri-build.mjs
 */

import { readFileSync, existsSync } from 'fs';
import { resolve, dirname } from 'path';
import { fileURLToPath } from 'url';
import { spawnTauri } from './ci-tauri-build.mjs';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, '..');

const featuresPath = resolve(ROOT, '.driver-features.json');
if (!existsSync(featuresPath)) {
  console.error('[e2e-tauri-build] missing .driver-features.json — run resolve-drivers / with-driver-inject first');
  process.exit(1);
}

const { features } = JSON.parse(readFileSync(featuresPath, 'utf-8'));
const featureList = ['webdriver', ...(Array.isArray(features) ? features : [])];
const args = ['build', '--debug', '-f', featureList.join(',')];

// Gate vite-gated E2E-only attributes (src/lib/tid.ts): the frontend build run by
// Tauri's beforeBuildCommand inherits this env, so webdriver builds render
// data-testid locators while plain `pnpm build` (no VITE_E2E) stays clean.
process.env.VITE_E2E = process.env.VITE_E2E || '1';

const result = spawnTauri(args, {
  log: (msg) => console.log(msg.replace('[ci-tauri-build]', '[e2e-tauri-build]')),
});
process.exit(result.status ?? 1);
