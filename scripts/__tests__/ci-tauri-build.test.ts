/** @vitest-environment node */
import { describe, expect, it } from 'vitest';
import { buildTauriArgs, pnpmBin, UPDATER_CONFIG } from '../ci-tauri-build.mjs';

describe('ci-tauri-build args', () => {
  it('passes updater config as parseable JSON with quoted keys', () => {
    const args = buildTauriArgs({ updater: true, target: 'aarch64-apple-darwin' });
    const configIdx = args.indexOf('--config');
    expect(configIdx).toBeGreaterThanOrEqual(0);
    const raw = args[configIdx + 1];
    expect(raw.startsWith('{')).toBe(true);
    expect(JSON.parse(raw)).toEqual(UPDATER_CONFIG);
    expect(raw).toContain('"bundle"');
    expect(raw).toContain('"createUpdaterArtifacts"');
  });

  it('omits --config unless updater is requested', () => {
    expect(buildTauriArgs({ features: ['plugin-redis'] })).toEqual([
      'tauri',
      'build',
      '-f',
      'plugin-redis',
    ]);
  });

  it('uses a real binary name rather than going through a shell', () => {
    expect(pnpmBin() === 'pnpm' || pnpmBin() === 'pnpm.cmd').toBe(true);
  });
});
