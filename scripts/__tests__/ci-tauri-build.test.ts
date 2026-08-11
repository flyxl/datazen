/** @vitest-environment node */
import { mkdtempSync, readFileSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';
import { describe, expect, it } from 'vitest';
import {
  buildTauriArgs,
  resolveTauriCli,
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
    expect(args).toEqual([
      'build',
      '--target',
      'x86_64-pc-windows-msvc',
      '--config',
      configPath,
    ]);
    expect(args[args.indexOf('--config') + 1].startsWith('{')).toBe(false);
  });

  it('writes updater config that Tauri can parse as JSON', () => {
    const dir = mkdtempSync(join(tmpdir(), 'datazen-ci-tauri-'));
    const file = writeUpdaterConfigFile(dir);
    expect(JSON.parse(readFileSync(file, 'utf-8'))).toEqual(UPDATER_CONFIG);
  });

  it('omits --config unless updater is requested', () => {
    expect(buildTauriArgs({ features: ['plugin-redis'] })).toEqual([
      'build',
      '-f',
      'plugin-redis',
    ]);
  });

  it('resolves the JS CLI entry instead of pnpm.cmd', () => {
    expect(resolveTauriCli().replaceAll('\\', '/')).toMatch(/@tauri-apps\/cli\/tauri\.js$/);
  });
});
