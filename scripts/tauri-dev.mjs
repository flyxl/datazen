#!/usr/bin/env node
/**
 * tauri-dev.mjs — wrapper for `pnpm tauri dev` that auto-resolves plugins
 *
 * Reads DATAZEN_PLUGINS env var or --plugins=xxx arg, resolves plugins,
 * then launches `tauri dev` with the correct Cargo features.
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

tauri.on('exit', (code) => process.exit(code ?? 0));
