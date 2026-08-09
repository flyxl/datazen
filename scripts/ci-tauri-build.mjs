#!/usr/bin/env node
/**
 * ci-tauri-build.mjs — run `pnpm tauri build` using .plugin-features.json.
 *
 * Used by CI inside with-plugin-inject so we never nest `bash -c` through
 * Node spawn (that loses the -c script argument on Windows).
 *
 * Usage:
 *   node scripts/ci-tauri-build.mjs --target=x86_64-pc-windows-msvc
 */

import { readFileSync, existsSync } from 'fs';
import { spawnSync } from 'child_process';
import { resolve, dirname } from 'path';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, '..');

const targetArg = process.argv.find((a) => a.startsWith('--target='));
const target = targetArg ? targetArg.slice('--target='.length) : null;

const featuresPath = resolve(ROOT, '.plugin-features.json');
if (!existsSync(featuresPath)) {
  console.error('[ci-tauri-build] missing .plugin-features.json — run resolve-drivers first');
  process.exit(1);
}

const { features } = JSON.parse(readFileSync(featuresPath, 'utf-8'));
const args = ['tauri', 'build'];
if (target) {
  args.push('--target', target);
}
if (process.argv.includes('--updater')) {
  args.push('--config', JSON.stringify({ bundle: { createUpdaterArtifacts: true } }));
}
if (Array.isArray(features) && features.length > 0) {
  args.push('-f', features.join(','));
}

console.log(`[ci-tauri-build] pnpm ${args.join(' ')}`);
const result = spawnSync('pnpm', args, {
  cwd: ROOT,
  stdio: 'inherit',
  shell: true,
  env: process.env,
});
process.exit(result.status ?? 1);
