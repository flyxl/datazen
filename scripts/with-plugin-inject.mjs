#!/usr/bin/env node
/**
 * with-plugin-inject.mjs — run a command after resolve-plugins, always restore stash after.
 *
 * Usage:
 *   node scripts/with-plugin-inject.mjs [--plugins=...] -- <cmd> [args...]
 *   node scripts/with-plugin-inject.mjs -- tauri build
 */

import { execSync, spawnSync } from 'child_process';
import { resolve, dirname } from 'path';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, '..');

const argv = process.argv.slice(2);
const sep = argv.indexOf('--');
const ahead = sep === -1 ? argv : argv.slice(0, sep);
const behind = sep === -1 ? [] : argv.slice(sep + 1);

const pluginsArgs = ahead.filter((a) => a.startsWith('--plugins'));
const resolveArgs = pluginsArgs.join(' ');

function restore() {
  try {
    execSync('node scripts/plugin-file-stash.mjs restore', {
      cwd: ROOT,
      stdio: 'inherit',
    });
  } catch {
    console.error('[with-plugin-inject] stash restore failed');
  }
}

execSync(`node scripts/resolve-plugins.mjs ${resolveArgs}`, {
  cwd: ROOT,
  stdio: 'inherit',
});

if (behind.length === 0) {
  restore();
  process.exit(0);
}

const result = spawnSync(behind[0], behind.slice(1), {
  cwd: ROOT,
  stdio: 'inherit',
  shell: true,
  env: process.env,
});

restore();
process.exit(result.status ?? 1);
