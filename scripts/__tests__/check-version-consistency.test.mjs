#!/usr/bin/env node
/**
 * [tester] check-version-consistency guard tests.
 *
 * Run: node scripts/__tests__/check-version-consistency.test.mjs
 */
import { cpSync, mkdtempSync, mkdirSync, writeFileSync } from 'fs';
import { tmpdir } from 'os';
import { dirname, join, resolve } from 'path';
import { fileURLToPath } from 'url';
import { spawnSync } from 'child_process';
import { describe, it, expect } from 'vitest';

const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const SCRIPT_SRC = join(REPO_ROOT, 'scripts/check-version-consistency.mjs');

function writeVersionFixture(root, { pkg, cargo, tauri }) {
  mkdirSync(join(root, 'scripts'), { recursive: true });
  mkdirSync(join(root, 'src-tauri'), { recursive: true });
  cpSync(SCRIPT_SRC, join(root, 'scripts/check-version-consistency.mjs'));
  writeFileSync(join(root, 'package.json'), JSON.stringify({ name: 'fixture', version: pkg }, null, 2));
  writeFileSync(
    join(root, 'src-tauri/Cargo.toml'),
    `[package]\nname = "fixture"\nversion = "${cargo}"\n`,
  );
  writeFileSync(
    join(root, 'src-tauri/tauri.conf.json'),
    JSON.stringify({ version: tauri }, null, 2),
  );
}

function runGuard(root) {
  return spawnSync(process.execPath, ['scripts/check-version-consistency.mjs'], {
    cwd: root,
    encoding: 'utf-8',
  });
}

describe('[tester] check-version-consistency', () => {
  it('test_tester_passes_when_all_sources_match', () => {
    const root = mkdtempSync(join(tmpdir(), 'ver-guard-ok-'));
    writeVersionFixture(root, { pkg: '1.2.3', cargo: '1.2.3', tauri: '1.2.3' });
    const result = runGuard(root);
    expect(result.status).toBe(0);
    expect(result.stdout).toMatch(/OK — all sources at 1\.2\.3/);
  });

  it('test_tester_exits_1_on_version_mismatch', () => {
    const root = mkdtempSync(join(tmpdir(), 'ver-guard-mismatch-'));
    writeVersionFixture(root, { pkg: '1.0.0', cargo: '1.0.0', tauri: '9.9.9' });
    const result = runGuard(root);
    expect(result.status).toBe(1);
    expect(result.stderr).toMatch(/Version mismatch detected/);
    expect(result.stderr).toMatch(/tauri\.conf\.json: 9\.9\.9/);
  });

  it('test_tester_fails_on_missing_version_field', () => {
    const root = mkdtempSync(join(tmpdir(), 'ver-guard-missing-'));
    mkdirSync(join(root, 'scripts'), { recursive: true });
    mkdirSync(join(root, 'src-tauri'), { recursive: true });
    cpSync(SCRIPT_SRC, join(root, 'scripts/check-version-consistency.mjs'));
    writeFileSync(join(root, 'package.json'), JSON.stringify({ name: 'fixture' }, null, 2));
    writeFileSync(join(root, 'src-tauri/Cargo.toml'), '[package]\nversion = "1.0.0"\n');
    writeFileSync(join(root, 'src-tauri/tauri.conf.json'), JSON.stringify({ version: '1.0.0' }));
    const result = runGuard(root);
    expect(result.status).not.toBe(0);
    expect(result.stderr).toMatch(/Could not read version from package\.json/);
  });

  it('test_tester_fails_on_invalid_semver', () => {
    const root = mkdtempSync(join(tmpdir(), 'ver-guard-invalid-'));
    writeVersionFixture(root, { pkg: 'not-semver', cargo: 'not-semver', tauri: 'not-semver' });
    const result = runGuard(root);
    expect(result.status).not.toBe(0);
    expect(result.stderr).toMatch(/Invalid semver in package\.json/);
  });
});
