#!/usr/bin/env node
/**
 * Pack every installable extension package under packages/extensions/ into a
 * zip for the plugin-install dialog.
 *
 * Source layout:   packages/extensions/<publisher>.<name>/
 * Output layout:   packages/extensions/dist/<publisher>.<name>.zip   (gitignored)
 *
 * Each package must carry a manifest.json whose `id` equals the directory name
 * (the plugin system enforces this on install). Packaging is copy-semantics:
 * the whole package dir is zipped, with `.DS_Store` excluded.
 *
 * Usage:
 *   node scripts/pack-extensions.mjs            # pack every package
 *   node scripts/pack-extensions.mjs datazen.playground   # a single package by id
 */

import { execFileSync } from 'child_process';
import { existsSync, mkdirSync, readdirSync, readFileSync, rmSync } from 'fs';
import { dirname, join, resolve } from 'path';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, '..');
const EXTENSIONS = resolve(ROOT, 'packages/extensions');
const OUT_DIR = join(EXTENSIONS, 'dist');

function listPackageDirs() {
  return readdirSync(EXTENSIONS, { withFileTypes: true })
    .filter((e) => e.isDirectory() && e.name !== 'dist')
    .map((e) => e.name)
    .sort();
}

function readId(pkgDir) {
  const manifestPath = join(EXTENSIONS, pkgDir, 'manifest.json');
  if (!existsSync(manifestPath)) return null;
  try {
    return JSON.parse(readFileSync(manifestPath, 'utf8')).id || null;
  } catch {
    return null;
  }
}

function pack(pkgDir, id) {
  const source = join(EXTENSIONS, pkgDir);
  const out = join(OUT_DIR, `${id}.zip`);

  if (!existsSync(OUT_DIR)) mkdirSync(OUT_DIR, { recursive: true });
  if (existsSync(out)) rmSync(out);

  try {
    // Zip the package directory contents (manifest.json at the zip root),
    // excluding macOS metadata files.
    execFileSync('zip', ['-r', '-q', out, '.', '-x', '.DS_Store'], { cwd: source, stdio: 'inherit' });
  } catch {
    console.error(`  !! failed to create ${out}; ensure the \`zip\` CLI is installed`);
    return false;
  }
  console.log(`  ${id}  ->  ${join('packages/extensions/dist', `${id}.zip`)}`);
  return true;
}

const all = listPackageDirs();
if (all.length === 0) {
  console.error(`No package directories under ${EXTENSIONS}.`);
  process.exit(1);
}

// Validate manifest `id` matches directory name (plugin host contract).
let invalid = false;
for (const pkgDir of all) {
  const id = readId(pkgDir);
  if (!id) {
    console.error(`  !! ${pkgDir}: missing or unreadable manifest.json (id required)`);
    invalid = true;
    continue;
  }
  if (id !== pkgDir) {
    console.error(`  !! ${pkgDir}: manifest id "${id}" != directory name (must match)`);
    invalid = true;
  }
}
if (invalid) {
  console.error('\nFix the mismatch above before packing. Aborting.');
  process.exit(1);
}

const requested = process.argv[2];
let targets = requested ? requested.split(',') : all.map(readId).filter(Boolean);

if (requested) {
  const unknown = targets.filter((t) => !all.includes(t));
  if (unknown.length) {
    console.error(`  !! unknown package id(s): ${unknown.join(', ')}`);
    process.exit(1);
  }
}

console.log(`Packing ${targets.length} extension${targets.length === 1 ? '' : 's'} to ${OUT_DIR}`);
let ok = 0;
for (const pkgDir of targets) {
  const id = readId(pkgDir);
  if (pack(pkgDir, id)) ok++;
}

console.log(ok === targets.length ? `Done. ${ok} package(s) written.` : `${ok}/${targets.length} packaged; see errors above.`);
if (ok !== targets.length) process.exit(1);