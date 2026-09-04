#!/usr/bin/env node
/**
 * Version consistency guard.
 *
 * Ensures the application semver stays in sync across:
 *   - package.json
 *   - src-tauri/Cargo.toml
 *   - src-tauri/tauri.conf.json
 *
 * Run: node scripts/check-version-consistency.mjs
 * Exit 0 when all match; exit 1 with a diff report otherwise.
 */
import { readFileSync } from 'fs';
import { dirname, resolve } from 'path';
import { fileURLToPath } from 'url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');

const VERSION_SOURCES = [
  {
    label: 'package.json',
    path: resolve(ROOT, 'package.json'),
    extract(text) {
      const parsed = JSON.parse(text);
      return parsed.version ?? null;
    },
  },
  {
    label: 'src-tauri/Cargo.toml',
    path: resolve(ROOT, 'src-tauri/Cargo.toml'),
    extract(text) {
      const match = text.match(/^version\s*=\s*"([^"]+)"/m);
      return match?.[1] ?? null;
    },
  },
  {
    label: 'src-tauri/tauri.conf.json',
    path: resolve(ROOT, 'src-tauri/tauri.conf.json'),
    extract(text) {
      const parsed = JSON.parse(text);
      return parsed.version ?? null;
    },
  },
];

function readVersion(source) {
  const text = readFileSync(source.path, 'utf-8');
  const version = source.extract(text);
  if (!version) {
    throw new Error(`Could not read version from ${source.label}`);
  }
  if (!/^\d+\.\d+\.\d+/.test(version)) {
    throw new Error(`Invalid semver in ${source.label}: ${version}`);
  }
  return version;
}

const versions = VERSION_SOURCES.map((source) => ({
  label: source.label,
  version: readVersion(source),
}));

const canonical = versions[0].version;
const mismatches = versions.filter((entry) => entry.version !== canonical);

if (mismatches.length > 0) {
  console.error('[check-version-consistency] Version mismatch detected:');
  for (const entry of versions) {
    const marker = entry.version === canonical ? '✓' : '✗';
    console.error(`  ${marker} ${entry.label}: ${entry.version}`);
  }
  console.error(
    `\nExpected all sources to match "${canonical}". Update via .cursor/skills/bump-version/scripts/bump.sh`,
  );
  process.exit(1);
}

console.log(`[check-version-consistency] OK — all sources at ${canonical}`);
