#!/usr/bin/env node
/**
 * Zip ThemePack sources under packages/themes/ for Settings → Import.
 *
 * Usage:
 *   node scripts/pack-community-theme.mjs community.dracula
 *   node scripts/pack-community-theme.mjs --all
 *
 * Output: packages/themes/dist/{id}.zip
 */
import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, readdirSync, rmSync, statSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const themesRoot = join(root, 'packages', 'themes');
const distDir = join(themesRoot, 'dist');

function listPackIds() {
  return readdirSync(themesRoot).filter((name) => {
    const dir = join(themesRoot, name);
    return statSync(dir).isDirectory() && existsSync(join(dir, 'manifest.json'));
  });
}

function packOne(id) {
  const src = join(themesRoot, id);
  if (!existsSync(join(src, 'manifest.json'))) {
    throw new Error(`Missing pack: ${src}`);
  }
  mkdirSync(distDir, { recursive: true });
  const out = join(distDir, `${id}.zip`);
  if (existsSync(out)) rmSync(out);
  // Zip contents of the pack dir (no extra top-level wrapper folder).
  execFileSync('zip', ['-r', '-X', out, '.'], { cwd: src, stdio: 'inherit' });
  console.log(`wrote ${out}`);
}

const arg = process.argv[2];
if (!arg || arg === '-h' || arg === '--help') {
  console.log('Usage: node scripts/pack-community-theme.mjs <packId|--all>');
  process.exit(arg ? 0 : 1);
}

if (arg === '--all') {
  for (const id of listPackIds()) packOne(id);
} else {
  packOne(arg);
}
