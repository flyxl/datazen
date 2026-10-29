import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

/**
 * Desktop updater flow tests (form-split F6 prep, moved from
 * `src/lib/__tests__/updater.test.ts` / `updaterVariant.test.ts`).
 *
 * The plugin-updater machinery is app-owned (`src/platform/desktop/updater.ts`);
 * these tests import the zone directly with the plugin mocks the flows ride.
 * Shared code reaches the same flows through the HostSettingsPort accessors in
 * `src/lib/updater.ts` — the lib-side tests cover the channel/manifest
 * reasoning and stay in `src/lib/__tests__/`.
 */

const mockCheck = vi.fn();
const mockDownloadAndInstall = vi.fn();
const mockRelaunch = vi.fn();

const envState = vi.hoisted(() => ({ desktop: false }));
vi.mock('@datazen/frontend/src/platform/env', () => ({
  get IS_DESKTOP_BUILD() {
    return envState.desktop;
  },
}));

vi.mock('@tauri-apps/plugin-updater', () => ({
  check: (...args: unknown[]) => mockCheck(...args),
}));

vi.mock('@tauri-apps/plugin-process', () => ({
  relaunch: (...args: unknown[]) => mockRelaunch(...args),
}));

// The Basic SKU: publishes a channel (the lib-side variant tests cover `custom`).
vi.mock('@datazen/frontend/src/extensions/generated', () => ({
  DATAZEN_VARIANT: 'basic',
  DATAZEN_UPDATER_CHANNEL: true,
}));

import { checkForUpdates, downloadAndInstallUpdate, maybeCheckOnStartup } from '../desktop/updater';

describe('updater flows without Tauri', () => {
  it('checkForUpdates returns error', async () => {
    const result = await checkForUpdates();
    expect(result).toEqual({ status: 'error', message: 'Updater is not available in this build' });
  });

  it('downloadAndInstallUpdate returns error', async () => {
    const result = await downloadAndInstallUpdate();
    expect(result.status).toBe('error');
  });
});

describe('updater flows with Tauri', () => {
  beforeEach(() => {
    envState.desktop = true;
    vi.clearAllMocks();
  });

  afterEach(() => {
    envState.desktop = false;
  });

  it('checkForUpdates returns upToDate when no update', async () => {
    mockCheck.mockResolvedValue(null);
    expect(await checkForUpdates()).toEqual({ status: 'upToDate' });
  });

  it('checkForUpdates returns available with version', async () => {
    mockCheck.mockResolvedValue({ version: '2.0.0' });
    expect(await checkForUpdates()).toEqual({ status: 'available', version: '2.0.0' });
  });

  it('checkForUpdates catches errors', async () => {
    mockCheck.mockRejectedValue(new Error('network fail'));
    const result = await checkForUpdates();
    expect(result).toEqual({ status: 'error', message: 'network fail' });
  });

  it('downloadAndInstallUpdate reports progress and relaunches', async () => {
    mockCheck.mockResolvedValue({
      version: '2.0.0',
      downloadAndInstall: mockDownloadAndInstall,
    });
    mockDownloadAndInstall.mockImplementation(
      async (cb: (e: { event: string; data: Record<string, number> }) => void) => {
        cb({ event: 'Started', data: { contentLength: 100 } });
        cb({ event: 'Progress', data: { chunkLength: 50 } });
        cb({ event: 'Progress', data: { chunkLength: 50 } });
        cb({ event: 'Finished', data: {} });
      },
    );
    mockRelaunch.mockResolvedValue(undefined);

    const progress: string[] = [];
    const result = await downloadAndInstallUpdate((p) => progress.push(p.phase));

    expect(progress).toEqual([
      'checking',
      'downloading',
      'downloading',
      'downloading',
      'installing',
      'done',
    ]);
    expect(result).toEqual({ status: 'installed', version: '2.0.0' });
    expect(mockRelaunch).toHaveBeenCalled();
  });

  it('downloadAndInstallUpdate returns upToDate when no update', async () => {
    mockCheck.mockResolvedValue(null);
    const progress: string[] = [];
    const result = await downloadAndInstallUpdate((p) => progress.push(p.phase));
    expect(result).toEqual({ status: 'upToDate' });
    expect(progress).toEqual(['checking', 'idle']);
  });

  it('downloadAndInstallUpdate handles errors', async () => {
    mockCheck.mockRejectedValue('boom');
    const progress: string[] = [];
    const result = await downloadAndInstallUpdate((p) => progress.push(p.phase));
    expect(result).toEqual({ status: 'error', message: 'boom' });
    expect(progress).toEqual(['checking', 'idle']);
  });

  it('maybeCheckOnStartup warns on error', async () => {
    mockCheck.mockRejectedValue(new Error('startup fail'));
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    await maybeCheckOnStartup(true);
    expect(warn).toHaveBeenCalledWith('[updater] startup check failed:', 'startup fail');
    warn.mockRestore();
  });
});

describe('updater flows refuse a manifest belonging to another SKU', () => {
  beforeEach(() => {
    envState.desktop = true;
    vi.clearAllMocks();
  });

  afterEach(() => {
    envState.desktop = false;
  });

  it('reports an error instead of offering another SKU update', async () => {
    mockCheck.mockResolvedValue({ version: '9.9.9', rawJson: { variant: 'all' } });
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});

    const result = await checkForUpdates();

    expect(result.status).toBe('error');
    expect(result.status === 'error' && result.message).toContain('different DataZen build');
    expect(warn).toHaveBeenCalledWith(
      '[updater] refusing update: manifest variant=all build variant=basic',
    );
    warn.mockRestore();
  });

  it('never downloads another SKU manifest, not even on the startup path', async () => {
    mockCheck.mockResolvedValue({
      version: '9.9.9',
      rawJson: { variant: 'akulaku' },
      downloadAndInstall: mockDownloadAndInstall,
    });
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});

    const progress: string[] = [];
    const result = await downloadAndInstallUpdate((p) => progress.push(p.phase));

    expect(result.status).toBe('error');
    expect(mockDownloadAndInstall).not.toHaveBeenCalled();
    expect(mockRelaunch).not.toHaveBeenCalled();
    expect(progress).toEqual(['checking', 'idle']);
    warn.mockRestore();
  });

  it('still installs when the manifest names this build', async () => {
    mockCheck.mockResolvedValue({
      version: '2.0.0',
      rawJson: { variant: 'basic' },
      downloadAndInstall: mockDownloadAndInstall,
    });
    mockDownloadAndInstall.mockResolvedValue(undefined);
    mockRelaunch.mockResolvedValue(undefined);

    expect(await downloadAndInstallUpdate()).toEqual({ status: 'installed', version: '2.0.0' });
  });
});

describe('updater flows on a SKU with no published channel', () => {
  // The reported bug (regression guard): a variant whose SKU has no channel
  // used to read Basic's manifest and install a Basic build over itself,
  // dropping every driver Basic does not ship. Nothing may reach the plugin.
  // The generated constants are re-mocked per test via doMock + a fresh
  // dynamic import of the zone module.
  beforeEach(() => {
    envState.desktop = true;
    vi.clearAllMocks();
    vi.resetModules();
    vi.doMock('@datazen/frontend/src/extensions/generated', () => ({
      DATAZEN_VARIANT: 'custom',
      DATAZEN_UPDATER_CHANNEL: false,
    }));
  });

  afterEach(() => {
    envState.desktop = false;
    vi.doUnmock('../../extensions/generated');
  });

  it('never reaches the plugin and answers unavailable', async () => {
    const { checkForUpdates: checkFlow, downloadAndInstallUpdate: installFlow } = await import(
      '../desktop/updater'
    );

    expect(await checkFlow()).toEqual({
      status: 'error',
      message: 'Updater is not available in this build',
    });
    expect(await installFlow()).toEqual({
      status: 'error',
      message: 'Updater is not available in this build',
    });
    expect(mockCheck).not.toHaveBeenCalled();
    expect(mockDownloadAndInstall).not.toHaveBeenCalled();
    expect(mockRelaunch).not.toHaveBeenCalled();
  });
});
