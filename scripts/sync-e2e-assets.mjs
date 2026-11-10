#!/usr/bin/env node
/**
 * Sync captured E2E assets from `e2e/screenshots/` into site assets.
 *
 * This is a post-capture step. It never re-runs capture/specs.
 *
 * Usage:
 *   node scripts/sync-e2e-assets.mjs --all
 *   node scripts/sync-e2e-assets.mjs --shots
 *   node scripts/sync-e2e-assets.mjs --blog
 *   node scripts/sync-e2e-assets.mjs --demo
 */
import fs from 'node:fs';
import path from 'node:path';
import url from 'node:url';

const ROOT = path.resolve(path.dirname(url.fileURLToPath(import.meta.url)), '..');
const args = new Set(process.argv.slice(2));
const all = args.has('--all') || (!args.has('--shots') && !args.has('--blog') && !args.has('--demo'));

function copyDirFiles(srcDir, destDir, exts) {
  if (!fs.existsSync(srcDir)) {
    console.warn(`[skip] source not found: ${path.relative(ROOT, srcDir)}`);
    return 0;
  }
  fs.mkdirSync(destDir, { recursive: true });
  let count = 0;
  for (const name of fs.readdirSync(srcDir)) {
    const src = path.join(srcDir, name);
    if (!fs.statSync(src).isFile()) continue;
    if (exts && !exts.some((e) => name.toLowerCase().endsWith(e))) continue;
    fs.copyFileSync(src, path.join(destDir, name));
    count++;
  }
  console.log(`[sync] ${path.relative(ROOT, srcDir)} -> ${path.relative(ROOT, destDir)} (${count} files)`);
  return count;
}

function copyFileIfExists(src, dest) {
  if (!fs.existsSync(src)) {
    console.warn(`[skip] source not found: ${path.relative(ROOT, src)}`);
    return false;
  }
  fs.mkdirSync(path.dirname(dest), { recursive: true });
  fs.copyFileSync(src, dest);
  console.log(`[sync] ${path.relative(ROOT, src)} -> ${path.relative(ROOT, dest)}`);
  return true;
}

if (all || args.has('--shots')) {
  copyDirFiles(
    path.join(ROOT, 'e2e', 'screenshots', 'site'),
    path.join(ROOT, 'site', 'assets', 'screenshots'),
    ['.png', '.jpg', '.jpeg', '.webp'],
  );
}

if (all || args.has('--blog')) {
  copyDirFiles(
    path.join(ROOT, 'e2e', 'screenshots', 'blog'),
    path.join(ROOT, 'site', 'assets', 'blog'),
    ['.png', '.jpg', '.jpeg', '.webp'],
  );
}

if (all || args.has('--demo')) {
  copyFileIfExists(
    path.join(ROOT, 'e2e', 'screenshots', 'demo', 'demo-recording.mp4'),
    path.join(ROOT, 'site', 'assets', 'video', 'demo-recording.mp4'),
  );
  // APNG/frames remain in e2e/screenshots/demo as capture artifacts
}
