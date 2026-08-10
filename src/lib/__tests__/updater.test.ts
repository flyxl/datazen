import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const mockCheck = vi.fn();
const mockDownloadAndInstall = vi.fn();
const mockRelaunch = vi.fn();

vi.mock('@tauri-apps/plugin-updater', () => ({
  check: (...args: unknown[]) => mockCheck(...args),
}));

vi.mock('@tauri-apps/plugin-process', () => ({
  relaunch: (...args: unknown[]) => mockRelaunch(...args),
}));

import {
  checkForUpdates,
  downloadAndInstallUpdate,
  isUpdaterSupported,
  maybeCheckOnStartup,
} from '../updater';

describe('updater without Tauri', () => {
  it('isUpdaterSupported returns false', () => {
    expect(isUpdaterSupported()).toBe(false);
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
    await maybeCheckOnStartup(true);
    expect(warn).not.toHaveBeenCalled();
    warn.mockRestore();
  });
});

describe('updater with Tauri', () => {
  beforeEach(() => {
    (globalThis as Record<string, unknown>).__TAURI_INTERNALS__ = {};
    vi.clearAllMocks();
  });

  afterEach(() => {
    delete (globalThis as Record<string, unknown>).__TAURI_INTERNALS__;
  });

  it('isUpdaterSupported returns true', () => {
    expect(isUpdaterSupported()).toBe(true);
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
    mockDownloadAndInstall.mockImplementation(async (cb: (e: { event: string; data: Record<string, number> }) => void) => {
      cb({ event: 'Started', data: { contentLength: 100 } });
      cb({ event: 'Progress', data: { chunkLength: 50 } });
      cb({ event: 'Progress', data: { chunkLength: 50 } });
      cb({ event: 'Finished', data: {} });
    });
    mockRelaunch.mockResolvedValue(undefined);

    const progress: string[] = [];
    const result = await downloadAndInstallUpdate((p) => progress.push(p.phase));

    expect(progress).toEqual(['checking', 'downloading', 'downloading', 'downloading', 'installing', 'done']);
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
    await maybeCheckOnStartup(false);
    expect(mockCheck).not.toHaveBeenCalled();
  });
});
