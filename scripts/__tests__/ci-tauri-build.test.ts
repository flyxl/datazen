/** @vitest-environment node */
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';
import { describe, expect, it } from 'vitest';
import {
  buildTauriArgs,
  checkProStagingReady,
  resolveTauriCli,
  REQUIRED_PRO_STAGED_PATHS,
  UPDATER_CONFIG,
  writeUpdaterConfigFile,
} from '../ci-tauri-build.mjs';

describe('ci-tauri-build args', () => {
  it('passes updater config as a JSON file path, not an inline object', () => {
    const configPath = join(tmpdir(), 'datazen-updater-test.json');
    const args = buildTauriArgs({
      updater: true,
      target: 'x86_64-pc-windows-msvc',
      updaterConfigPath: configPath,
    });
    expect(args).toEqual(['build', '--target', 'x86_64-pc-windows-msvc', '--config', configPath]);
    expect(args[args.indexOf('--config') + 1].startsWith('{')).toBe(false);
  });

  it('writes updater config that Tauri can parse as JSON', () => {
    const dir = mkdtempSync(join(tmpdir(), 'datazen-ci-tauri-'));
    const file = writeUpdaterConfigFile(dir);
    expect(JSON.parse(readFileSync(file, 'utf-8'))).toEqual(UPDATER_CONFIG);
  });

  it('omits --config unless updater is requested', () => {
    expect(buildTauriArgs({ features: ['driver-redis'] })).toEqual(['build', '-f', 'driver-redis']);
  });

  it('resolves the JS CLI entry instead of pnpm.cmd', () => {
    expect(resolveTauriCli().replaceAll('\\', '/')).toMatch(/@tauri-apps\/cli\/tauri\.js$/);
  });

  it('generates Pro config when edition=pro is requested', () => {
    const dir = mkdtempSync(join(tmpdir(), 'datazen-ci-tauri-'));
    const args = buildTauriArgs({
      edition: 'pro',
      features: ['driver-redis'],
      updaterConfigPath: join(dir, 'test-pro.json'),
    });
    expect(args).toEqual(['build', '--config', join(dir, 'test-pro.json'), '-f', 'driver-redis']);
  });
});

describe('ci-tauri-build pro staging preflight', () => {
  it('exposes the required staged file list', () => {
    expect(REQUIRED_PRO_STAGED_PATHS).toEqual([
      'manifest.json',
      'dist/index.esm.js',
      'signature.sig',
    ]);
  });

  it('reports an empty missing list when the staged tree is complete', () => {
    const root = mkdtempSync(join(tmpdir(), 'datazen-pro-ready-'));
    const staging = join(root, 'src-tauri', 'resources', 'builtin-ep', 'sql-editor-pro');
    mkdirSync(join(staging, 'dist'), { recursive: true });
    writeFileSync(join(staging, 'manifest.json'), '{}');
    writeFileSync(join(staging, 'dist/index.esm.js'), '// bundle');
    writeFileSync(join(staging, 'signature.sig'), '{}');
    expect(checkProStagingReady({ root, log: () => {} })).toEqual([]);
  });

  it('reports missing files and emits a ::notice:: line when staging is absent', () => {
    const notices = [];
    const root = mkdtempSync(join(tmpdir(), 'datazen-pro-missing-'));
    const missing = checkProStagingReady({ root, log: (m) => notices.push(m) });
    expect(missing).toEqual(REQUIRED_PRO_STAGED_PATHS);
    expect(notices.some((m) => m.startsWith('::notice::'))).toBe(true);
  });
});
