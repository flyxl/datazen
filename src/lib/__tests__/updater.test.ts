import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const mockCheck = vi.fn();
const mockDownloadAndInstall = vi.fn();
const mockRelaunch = vi.fn();

// Form-split F2: updater's form branch is the compile-time IS_DESKTOP_BUILD
// constant; a getter-export mock replaces the old __TAURI_INTERNALS__ toggle.
const envState = vi.hoisted(() => ({ desktop: false }));
vi.mock('../../platform/desktop/env', () => ({
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

// The SKU is a build-time constant from gitignored codegen. These suites stand in
// for "this artifact was built as the Basic SKU, which publishes a channel" — the
// gating of every other SKU is covered in updaterVariant.test.ts.
vi.mock('../../extensions/generated', () => ({
  DATAZEN_VARIANT: 'basic',
  DATAZEN_UPDATER_CHANNEL: true,
}));

import {
  checkForUpdates,
  currentVariant,
  downloadAndInstallUpdate,
  getUpdateChannel,
  hasUpdaterChannel,
  isUpdaterSupported,
  manifestBelongsToBuild,
  maybeCheckOnStartup,
  readManifestVariant,
} from '../updater';

describe('updater without Tauri', () => {
  it('isUpdaterSupported returns false', () => {
    expect(isUpdaterSupported()).toBe(false);
  });

  it('reports no update channel for a non-desktop build', () => {
    expect(getUpdateChannel()).toBe('none');
  });

  it('checkForUpdates returns error', async () => {
    const result = await checkForUpdates();
    expect(result).toEqual({ status: 'error', message: 'Updater is not available in this build' });
  });

  it('downloadAndInstallUpdate returns error', async () => {
    const result = await downloadAndInstallUpdate();
    expect(result.status).toBe('error');
  });

  it('maybeCheckOnStartup is no-op', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const info = vi.spyOn(console, 'info').mockImplementation(() => {});
    await maybeCheckOnStartup(true);
    expect(warn).not.toHaveBeenCalled();
    // Browser dev: no channel at all, so nothing to check.
    expect(info).toHaveBeenCalledWith(
      '[updater] startup check skipped: setting=true channel=none variant=basic',
    );
    warn.mockRestore();
    info.mockRestore();
  });
});

describe('updater with Tauri', () => {
  beforeEach(() => {
    envState.desktop = true;
    vi.clearAllMocks();
  });

  afterEach(() => {
    envState.desktop = false;
  });

  it('isUpdaterSupported returns true', () => {
    expect(isUpdaterSupported()).toBe(true);
  });

  it('exposes the build SKU and its channel', () => {
    expect(currentVariant()).toBe('basic');
    expect(hasUpdaterChannel()).toBe(true);
    expect(getUpdateChannel()).toBe('auto');
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

  it('maybeCheckOnStartup skips when disabled', async () => {
    const info = vi.spyOn(console, 'info').mockImplementation(() => {});
    await maybeCheckOnStartup(false);
    expect(mockCheck).not.toHaveBeenCalled();
    // The decision is logged so "why is this build not checking for updates?"
    // can be answered from the console.
    expect(info).toHaveBeenCalledWith(
      '[updater] startup check skipped: setting=false channel=auto variant=basic',
    );
    info.mockRestore();
  });
});

describe('updater refuses a manifest belonging to another SKU', () => {
  beforeEach(() => {
    envState.desktop = true;
    vi.clearAllMocks();
  });

  afterEach(() => {
    envState.desktop = false;
  });

  it('reads the manifest variant only when it is a usable string', () => {
    expect(readManifestVariant({ variant: 'all' })).toBe('all');
    expect(readManifestVariant({})).toBeNull();
    expect(readManifestVariant({ variant: 7 })).toBeNull();
    expect(readManifestVariant({ variant: '   ' })).toBeNull();
    expect(readManifestVariant(null)).toBeNull();
    expect(readManifestVariant('all')).toBeNull();
  });

  it('accepts a manifest only when it names this build', () => {
    // Basic inherits its manifest name from earlier releases, so manifests
    // published before the field existed must stay acceptable — rejecting them
    // would strand every already-installed Basic build.
    expect(manifestBelongsToBuild({}, 'basic')).toBe(true);
    expect(manifestBelongsToBuild({ variant: 'basic' }, 'basic')).toBe(true);
    expect(manifestBelongsToBuild({ variant: 'all' }, 'basic')).toBe(false);

    // Variant manifests exist only because of per-SKU channels, so a missing
    // field there can only mean the manifest belongs to another SKU.
    expect(manifestBelongsToBuild({}, 'all')).toBe(false);
    expect(manifestBelongsToBuild({ variant: 'all' }, 'all')).toBe(true);
    expect(manifestBelongsToBuild({ variant: '-all' }, 'all')).toBe(true);
    expect(manifestBelongsToBuild({ variant: ' AKULAKU ' }, 'akulaku')).toBe(true);
    expect(manifestBelongsToBuild({ variant: 'akulaku' }, 'all')).toBe(false);

    // This predicate only compares SKUs; `custom` never gets this far because
    // `getUpdateChannel()` returns 'manual' for it.
    expect(manifestBelongsToBuild({}, 'custom')).toBe(false);
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
