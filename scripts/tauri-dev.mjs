#!/usr/bin/env node
/**
 * tauri-dev.mjs — wrapper for `pnpm tauri:dev` that resolves drivers then
 * restores stashed clean managed files on exit.
 *
 * Usage:
 *   pnpm tauri:dev                         # basic (postgres/mysql/sqlite/redis)
 *   pnpm tauri:dev --drivers=all                 # all path drivers (no git)
 *   pnpm tauri:dev --drivers=basic,kiwi,superset # basic expander + listed git drivers
 *   pnpm tauri:dev --drivers=all,kiwi,superset   # all path + listed git drivers
 *   pnpm tauri:dev --drivers=kiwi                # only kiwi (+ no path drivers unless listed)
 *   DATAZEN_DRIVERS=all pnpm tauri:dev           # env var also works
 */

import { execSync, spawn } from 'child_process';
import { resolve, dirname } from 'path';
import { readFileSync } from 'fs';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, '..');

function restoreStash() {
  console.log('[tauri:dev] restoring managed files from copy-stash...');
  try {
    execSync('node scripts/plugin-file-stash.mjs restore', {
      cwd: ROOT,
      stdio: 'inherit',
    });
  } catch (e) {
    console.error('[tauri:dev] stash restore failed — working tree may still have injected files');
  }
}

const args = process.argv.slice(2);
if (args.some((a) => a === '--plugins' || a.startsWith('--plugins=')) || process.env.DATAZEN_PLUGINS) {
  console.error('[tauri:dev] --plugins / DATAZEN_PLUGINS are no longer supported. Use --drivers=... or DATAZEN_DRIVERS.');
  process.exit(1);
}

const driversArgs = args.filter((a) => a.startsWith('--drivers'));
const otherArgs = args.filter((a) => !a.startsWith('--drivers'));
const driversStr = driversArgs.join(' ');

console.log('[tauri:dev] generating menu labels from locales...');
execSync('node scripts/generate-menu-labels.mjs', {
  cwd: ROOT,
  stdio: 'inherit',
});

console.log('[tauri:dev] resolving drivers (copy-stash + inject)...');
execSync(`node scripts/resolve-drivers.mjs ${driversStr}`, {
  cwd: ROOT,
  stdio: 'inherit',
});

const featuresFile = resolve(ROOT, '.plugin-features.json');
const { features } = JSON.parse(readFileSync(featuresFile, 'utf-8'));

const tauriArgs = ['tauri', 'dev'];

if (features.length > 0) {
  tauriArgs.push('-f', features.join(','));
}

tauriArgs.push(...otherArgs);

console.log(`[tauri:dev] running: npx ${tauriArgs.join(' ')}`);

if (process.env.DATAZEN_KEYRING) {
  console.log(`[tauri:dev] DATAZEN_KEYRING=${process.env.DATAZEN_KEYRING}`);
} else {
  console.log('[tauri:dev] DATAZEN_KEYRING unset → Rust will use .key on adhoc/unsigned macOS builds');
}

const tauri = spawn('npx', tauriArgs, {
  cwd: ROOT,
  stdio: 'inherit',
  shell: true,
  env: {
    ...process.env,
    DATAZEN_DRIVERS:
      process.env.DATAZEN_DRIVERS ||
      driversArgs.map((a) => a.split('=')[1]).join(',') ||
      'basic',
  },
});

process.on('SIGINT', () => {
  restoreStash();
  process.exit(130);
});
process.on('SIGTERM', () => {
  restoreStash();
  process.exit(143);
});

tauri.on('exit', (code) => {
  restoreStash();
  process.exit(code ?? 0);
});
