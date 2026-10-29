/** @vitest-environment node */
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';
import { describe, expect, it } from 'vitest';
import {
  BUILD_PROFILE,
  buildTauriArgs,
  checkProStagingReady,
  resolveTauriCli,
  REQUIRED_PRO_STAGED_PATHS,
  UPDATER_CONFIG,
  updaterEndpointsForVariant,
  writeTauriConfigFile,
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

  it('never passes --profile, which tauri-cli 2.10.1 rejects', () => {
    // Regression guard: `tauri build` in tauri-cli 2.10.1 has no `--profile`
    // flag at all (only `--debug` vs release). Emitting it aborts every matrix
    // job in under a second with `error: unexpected argument '--profile' found`,
    // and it cannot be worked around by a different flag name — a custom Cargo
    // profile simply is not reachable through `tauri build`.
    const args = buildTauriArgs({
      target: 'aarch64-apple-darwin',
      updater: true,
      updaterConfigPath: '/tmp/updater.json',
    });
    expect(args).toEqual([
      'build',
      '--target',
      'aarch64-apple-darwin',
      '--config',
      '/tmp/updater.json',
    ]);
  });

  it('agrees with BUILD_PROFILE on the directory tauri build actually writes to', () => {
    // upx-compress and the release workflow's bundle paths look under
    // target/<triple>/<profile>; if that ever drifts from what tauri builds,
    // the matrix looks for binaries in a directory that never gets written.
    expect(BUILD_PROFILE).toBe('release');
  });
});

describe('ci-tauri-build typecheck-once override', () => {
  it('emits a build config even when no updater artifacts are requested', () => {
    // The only reason to pass --config here is the beforeBuildCommand
    // override, so omitting it would silently keep the slow pnpm build.
    const dir = mkdtempSync(join(tmpdir(), 'datazen-ci-tauri-'));
    const args = buildTauriArgs({
      features: ['driver-redis'],
      beforeBuildCommand: 'pnpm build:bundle',
    });
    expect(args[0]).toBe('build');
    expect(args).toContain('--config');
    const config = JSON.parse(readFileSync(args[args.indexOf('--config') + 1], 'utf-8'));
    expect(config).toEqual({ build: { beforeBuildCommand: 'pnpm build:bundle' } });

    const file = writeTauriConfigFile({ beforeBuildCommand: 'pnpm build:bundle', dir });
    expect(file).toContain('-fastfe');
  });

  it('keeps the updater artifact flag alongside the override', () => {
    const file = writeTauriConfigFile({
      updater: true,
      beforeBuildCommand: 'pnpm build:bundle',
    });
    expect(JSON.parse(readFileSync(file, 'utf-8'))).toEqual({
      ...UPDATER_CONFIG,
      build: { beforeBuildCommand: 'pnpm build:bundle' },
    });
  });
});

describe('ci-tauri-build updater endpoint per SKU', () => {
  it('leaves Basic on the endpoint compiled into tauri.conf.json', () => {
    // Already-installed Basic builds have that URL baked in, and Basic is the
    // default channel, so its build must stay byte-identical.
    expect(updaterEndpointsForVariant('basic')).toBeNull();
    expect(updaterEndpointsForVariant(undefined)).toBeNull();
    expect(updaterEndpointsForVariant('custom')).toBeNull();
    const file = writeTauriConfigFile({ updater: true, isPro: true, variant: 'basic' });
    expect(JSON.parse(readFileSync(file, 'utf-8'))).toEqual(UPDATER_CONFIG);
  });

  it('points each variant at its own manifest', () => {
    expect(updaterEndpointsForVariant('all')).toEqual([
      'https://github.com/flyxl/datazen/releases/latest/download/latest-all.json',
    ]);
    expect(updaterEndpointsForVariant('akulaku')).toEqual([
      'https://github.com/flyxl/datazen/releases/latest/download/latest-akulaku.json',
    ]);
    // The release matrix spells it `-all`; both must resolve to one channel.
    expect(updaterEndpointsForVariant('-all')).toEqual(updaterEndpointsForVariant('all'));
  });

  it('writes an overlay that merges into plugins.updater rather than replacing it', () => {
    // Tauri merges --config deeply (arrays replace, objects merge), so emitting
    // only `endpoints` keeps pubkey / windows settings from the base config.
    const dir = mkdtempSync(join(tmpdir(), 'datazen-ci-tauri-'));
    const file = writeTauriConfigFile({ updater: true, isPro: false, variant: 'all', dir });
    expect(JSON.parse(readFileSync(file, 'utf-8'))).toEqual({
      ...UPDATER_CONFIG,
      plugins: {
        updater: {
          endpoints: ['https://github.com/flyxl/datazen/releases/latest/download/latest-all.json'],
        },
      },
    });
  });

  it('forces a config file for a variant even without --updater', () => {
    // The endpoint must never point at another SKU's manifest, whether or not
    // this particular build is producing updater artifacts.
    const args = buildTauriArgs({ features: ['driver-redis'], edition: 'pro', variant: 'all' });
    expect(args).toContain('--config');
    const config = JSON.parse(readFileSync(args[args.indexOf('--config') + 1], 'utf-8'));
    expect(config.plugins.updater.endpoints[0]).toContain('latest-all.json');

    // Basic still relies on the base config when nothing else needs an overlay.
    expect(buildTauriArgs({ features: ['driver-redis'], variant: 'basic' })).toEqual([
      'build',
      '-f',
      'driver-redis',
    ]);
  });

  it('keeps an explicit updaterConfigPath override winning over the variant overlay', () => {
    const args = buildTauriArgs({
      variant: 'all',
      updater: true,
      updaterConfigPath: '/tmp/explicit.json',
    });
    expect(args[args.indexOf('--config') + 1]).toBe('/tmp/explicit.json');
  });

  it('names config files per variant so two SKUs cannot share one overlay', () => {
    const dir = mkdtempSync(join(tmpdir(), 'datazen-ci-tauri-'));
    const allFile = writeTauriConfigFile({ isPro: true, variant: 'all', dir });
    const akulakuFile = writeTauriConfigFile({ isPro: true, variant: 'akulaku', dir });
    expect(allFile).not.toBe(akulakuFile);
    expect(allFile).toContain('-all');
    expect(akulakuFile).toContain('-akulaku');
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
    const staging = join(root, 'apps/desktop/src-tauri', 'resources', 'builtin-ep', 'sql-editor-pro');
    mkdirSync(join(staging, 'dist'), { recursive: true });
    writeFileSync(join(staging, 'manifest.json'), '{}');
    writeFileSync(join(staging, 'dist/index.esm.js'), '// bundle');
    writeFileSync(join(staging, 'signature.sig'), '{}');
    expect(checkProStagingReady({ root, log: () => {} })).toEqual([]);
  });

  it('reports missing files and emits a ::notice:: line when staging is absent', () => {
    const notices: string[] = [];
    const root = mkdtempSync(join(tmpdir(), 'datazen-pro-missing-'));
    const missing = checkProStagingReady({ root, log: (m) => notices.push(m) });
    expect(missing).toEqual(REQUIRED_PRO_STAGED_PATHS);
    expect(notices.some((m) => m.startsWith('::notice::'))).toBe(true);
  });
});
