#!/usr/bin/env node
/**
 * dev.mjs — wrapper for `pnpm dev` that forwards --plugins to resolve-plugins.mjs
 *
 * Usage:
 *   pnpm dev                        # all plugins (default)
 *   pnpm dev --plugins=kiwi         # only kiwi
 *   pnpm dev --plugins=kiwi,olap    # kiwi + olap
 *   pnpm dev --plugins=none         # no plugins
 *   DATAZEN_PLUGINS=kiwi pnpm dev   # env var also works
 */

import { execSync, spawn } from 'child_process';
import { resolve, dirname } from 'path';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, '..');

const args = process.argv.slice(2);
const pluginsArgs = args.filter(a => a.startsWith('--plugins'));

const resolveArgs = pluginsArgs.length > 0
  ? pluginsArgs.join(' ')
  : process.env.DATAZEN_PLUGINS
    ? `--plugins=${process.env.DATAZEN_PLUGINS}`
    : '';

console.log('[dev] resolving plugins...');
execSync(`node scripts/resolve-plugins.mjs ${resolveArgs}`, {
  cwd: ROOT,
  stdio: 'inherit',
});

console.log('[dev] starting vite...');
const vite = spawn('npx', ['vite'], {
  cwd: ROOT,
  stdio: 'inherit',
  shell: true,
});

vite.on('exit', (code) => process.exit(code ?? 0));
