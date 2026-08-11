#!/usr/bin/env node
/**
 * Build Tauri v2 updater `latest.json` from Basic GitHub Release assets.
 *
 * Expects a directory of renamed Basic artifacts (no -all / -akulaku), including
 * updater bundles + matching `.sig` files:
 *   - darwin-aarch64: *-macos-arm64.tar.gz
 *   - darwin-x86_64:  *-macos-x64.tar.gz
 *   - windows-x86_64: *-windows-x64.exe (NSIS; prefer non-.msi)
 *   - linux-x86_64:   *-linux-x64.AppImage
 *
 * Usage:
 *   node scripts/generate-updater-latest-json.mjs \
 *     --assets-dir ./assets \
 *     --version 0.0.9 \
 *     --tag v0.0.9 \
 *     --repo flyxl/datazen \
 *     --out ./latest.json
 */
import fs from 'node:fs';
import path from 'node:path';

function arg(name, fallback) {
  const i = process.argv.indexOf(`--${name}`);
  if (i >= 0 && process.argv[i + 1]) return process.argv[i + 1];
  return fallback;
}

const assetsDir = path.resolve(arg('assets-dir', 'assets'));
const version = String(arg('version', '')).replace(/^v/, '');
const tag = arg('tag', version ? `v${version}` : '');
const repo = arg('repo', 'flyxl/datazen');
const outPath = path.resolve(arg('out', 'latest.json'));
const notes = arg('notes', '');

if (!version || !tag) {
  console.error('Missing --version and/or --tag');
  process.exit(1);
}
if (!fs.existsSync(assetsDir)) {
  console.error(`Assets dir not found: ${assetsDir}`);
  process.exit(1);
}

const files = fs.readdirSync(assetsDir).filter((f) => !f.startsWith('.'));

function pick(predicate) {
  const matches = files.filter(predicate).filter((f) => !f.includes('-all.') && !f.includes('-akulaku.'));
  return matches.sort()[0] ?? null;
}

function readSig(bundleName) {
  const sigName = `${bundleName}.sig`;
  const sigPath = path.join(assetsDir, sigName);
  if (!fs.existsSync(sigPath)) {
    throw new Error(`Missing signature for ${bundleName} (expected ${sigName})`);
  }
  return fs.readFileSync(sigPath, 'utf8').trim();
}

function platformUrl(fileName) {
  return `https://github.com/${repo}/releases/download/${tag}/${fileName}`;
}

const mapping = [
  {
    key: 'darwin-aarch64',
    file: pick((f) => f.endsWith('-macos-arm64.tar.gz') || f.endsWith('_aarch64.app.tar.gz')),
  },
  {
    key: 'darwin-x86_64',
    file: pick((f) => f.endsWith('-macos-x64.tar.gz') || (f.endsWith('.app.tar.gz') && f.includes('x64') && !f.includes('arm64'))),
  },
  {
    key: 'windows-x86_64',
    file: pick((f) => f.endsWith('-windows-x64.exe') && !f.endsWith('.msi')),
  },
  {
    key: 'linux-x86_64',
    file: pick((f) => f.endsWith('-linux-x64.AppImage')),
  },
];

const platforms = {};
const missing = [];

for (const { key, file } of mapping) {
  if (!file) {
    missing.push(key);
    continue;
  }
  platforms[key] = {
    url: platformUrl(file),
    signature: readSig(file),
  };
}

if (Object.keys(platforms).length === 0) {
  console.error('No updater platforms found in assets. Files:', files.join(', '));
  process.exit(1);
}

if (missing.length) {
  console.warn(`Warning: missing platforms: ${missing.join(', ')}`);
}

const latest = {
  version,
  notes,
  pub_date: new Date().toISOString(),
  platforms,
};

fs.writeFileSync(outPath, `${JSON.stringify(latest, null, 2)}\n`);
console.log(`Wrote ${outPath} with platforms: ${Object.keys(platforms).join(', ')}`);
