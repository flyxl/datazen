#!/usr/bin/env node
/**
 * tauri-dev.mjs — wrapper for `pnpm tauri:dev` that resolves plugins then
 * restores stashed clean managed files on exit.
 *
 * Usage:
 *   pnpm tauri:dev                       # all plugins (default)
 *   pnpm tauri:dev --plugins=kiwi        # only kiwi
 *   pnpm tauri:dev --plugins=none        # no plugins
 *   DATAZEN_PLUGINS=kiwi pnpm tauri:dev  # env var also works
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
const pluginsArgs = args.filter(a => a.startsWith('--plugins'));
const otherArgs = args.filter(a => !a.startsWith('--plugins'));
const pluginsStr = pluginsArgs.join(' ');

console.log('[tauri:dev] generating menu labels from locales...');
execSync('node scripts/generate-menu-labels.mjs', {
  cwd: ROOT,
  stdio: 'inherit',
});

console.log('[tauri:dev] resolving plugins (copy-stash + inject)...');
execSync(`node scripts/resolve-plugins.mjs ${pluginsStr}`, {
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

// Do not force DATAZEN_KEYRING=file here: macOS adhoc/unsigned binaries already
// auto-select the `.key` backend in Rust (with a one-time Keychain→file export).
// Explicit `DATAZEN_KEYRING=file` skips Keychain entirely (CI); `=keyring` forces it.
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
    DATAZEN_PLUGINS: process.env.DATAZEN_PLUGINS || pluginsArgs.map(a => a.split('=')[1]).join(',') || 'all',
  },
});

process.on('SIGINT', () => { restoreStash(); process.exit(130); });
process.on('SIGTERM', () => { restoreStash(); process.exit(143); });

tauri.on('exit', (code) => {
  restoreStash();
  process.exit(code ?? 0);
});
