#!/usr/bin/env node
/**
 * tauri-dev.mjs — wrapper for `pnpm tauri dev` that auto-resolves plugins
 *
 * Reads DATAZEN_PLUGINS env var or --plugins=xxx arg, resolves plugins,
 * then launches `tauri dev` with the correct Cargo features.
 * On exit, restores Cargo.toml files to their clean (no-plugin) state.
 *
 * Usage:
 *   pnpm tauri:dev                       # all plugins (default)
 *   pnpm tauri:dev --plugins=kiwi        # only kiwi
 *   pnpm tauri:dev --plugins=none        # no plugins
 *   DATAZEN_PLUGINS=kiwi pnpm tauri:dev  # env var also works
 */

import { execSync, spawn } from 'child_process';
import { resolve, dirname } from 'path';
import { readFileSync, writeFileSync } from 'fs';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, '..');

const CARGO_TOML = resolve(ROOT, 'Cargo.toml');
const TAURI_CARGO_TOML = resolve(ROOT, 'src-tauri/Cargo.toml');

// Save original Cargo.toml contents before injection
const originalCargoToml = readFileSync(CARGO_TOML, 'utf-8');
const originalTauriCargoToml = readFileSync(TAURI_CARGO_TOML, 'utf-8');

function restoreCargoFiles() {
  console.log('[tauri:dev] restoring Cargo.toml files...');
  writeFileSync(CARGO_TOML, originalCargoToml);
  writeFileSync(TAURI_CARGO_TOML, originalTauriCargoToml);
}

const args = process.argv.slice(2);
const pluginsArgs = args.filter(a => a.startsWith('--plugins'));
const otherArgs = args.filter(a => !a.startsWith('--plugins'));
const pluginsStr = pluginsArgs.join(' ');

console.log('[tauri:dev] resolving plugins...');
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
const tauri = spawn('npx', tauriArgs, {
  cwd: ROOT,
  stdio: 'inherit',
  shell: true,
  env: {
    ...process.env,
    DATAZEN_PLUGINS: process.env.DATAZEN_PLUGINS || pluginsArgs.map(a => a.split('=')[1]).join(',') || 'all',
  },
});

// Restore Cargo.toml on process exit (normal exit, SIGINT, SIGTERM)
process.on('SIGINT', () => { restoreCargoFiles(); process.exit(130); });
process.on('SIGTERM', () => { restoreCargoFiles(); process.exit(143); });

tauri.on('exit', (code) => {
  restoreCargoFiles();
  process.exit(code ?? 0);
});
