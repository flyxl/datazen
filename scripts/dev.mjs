#!/usr/bin/env node
/**
 * dev.mjs — wrapper for `pnpm dev` that resolves drivers then restores stash on exit.
 *
 * Usage:
 *   pnpm dev                          # all path drivers (default; no git)
 *   pnpm dev --drivers=kiwi           # only kiwi
 *   pnpm dev --drivers=basic          # four core path drivers
 *   DATAZEN_DRIVERS=basic pnpm dev    # env var also works
 */

import { execSync, spawn } from 'child_process';
import { resolve, dirname } from 'path';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, '..');

function restoreStash() {
  console.log('[dev] restoring managed files from copy-stash...');
  try {
    execSync('node scripts/plugin-file-stash.mjs restore', {
      cwd: ROOT,
      stdio: 'inherit',
    });
  } catch {
    console.error('[dev] stash restore failed — working tree may still have injected files');
  }
}

const args = process.argv.slice(2);
if (args.some((a) => a === '--plugins' || a.startsWith('--plugins=')) || process.env.DATAZEN_PLUGINS) {
  console.error('[dev] --plugins / DATAZEN_PLUGINS are no longer supported. Use --drivers=... or DATAZEN_DRIVERS.');
  process.exit(1);
}

const driversArgs = args.filter((a) => a.startsWith('--drivers'));

const resolveArgs =
  driversArgs.length > 0
    ? driversArgs.join(' ')
    : process.env.DATAZEN_DRIVERS
      ? `--drivers=${process.env.DATAZEN_DRIVERS}`
      : '';

console.log('[dev] resolving drivers (stash + inject)...');
execSync(`node scripts/resolve-drivers.mjs ${resolveArgs}`, {
  cwd: ROOT,
  stdio: 'inherit',
});

console.log('[dev] starting vite...');
const vite = spawn('npx', ['vite'], {
  cwd: ROOT,
  stdio: 'inherit',
  shell: true,
});

process.on('SIGINT', () => {
  restoreStash();
  process.exit(130);
});
process.on('SIGTERM', () => {
  restoreStash();
  process.exit(143);
});

vite.on('exit', (code) => {
  restoreStash();
  process.exit(code ?? 0);
});
