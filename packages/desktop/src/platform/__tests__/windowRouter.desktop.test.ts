import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

/**
 * Desktop `WindowRouter` tests (form-split F6 prep, moved from
 * `src/lib/__tests__/windowManager.test.ts`'s window-machinery assertions).
 *
 * The zone owns the low-level half: create_sub_window on the Tauri rail, the
 * browser-tab fallback for browser-dev runs, singleton dedup, focus-existing
 * and the error dialog. The high-level helper contract is covered by the
 * lib-side test through the port.
 */

const mockInvoke = vi.fn().mockResolvedValue(undefined);
const mockMessage = vi.fn().mockResolvedValue(undefined);
const mockShow = vi.fn().mockResolvedValue(undefined);
const mockUnminimize = vi.fn().mockResolvedValue(undefined);
const mockSetFocus = vi.fn().mockResolvedValue(undefined);

const envState = vi.hoisted(() => ({ desktop: false }));
vi.mock('@datazen/frontend/testing/platform/env.ts', () => ({
  get IS_DESKTOP_BUILD() {
    return envState.desktop;
  },
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({
  message: (...args: unknown[]) => mockMessage(...args),
}));

const mockGetByLabel = vi.fn().mockResolvedValue(null);

vi.mock('@tauri-apps/api/webviewWindow', () => ({
  WebviewWindow: {
    getByLabel: (...args: unknown[]) => mockGetByLabel(...args),
    getAll: vi.fn().mockResolvedValue([]),
  },
}));

import {
  hasOpenChildWindows,
  openMultiInstanceWindow,
  openSingletonWindow,
} from '../desktop/windowRouter';

describe('desktop windowRouter — browser fallback', () => {
  beforeEach(() => {
    envState.desktop = false;
    vi.clearAllMocks();
    vi.spyOn(window, 'open').mockReturnValue(null);
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('openSingletonWindow opens a browser tab with the serialized params', () => {
    openSingletonWindow('data-sync-singleton', {
      params: {
        window: 'data-sync',
        sourceId: 'pg-src',
        targetId: 'pg-tgt',
        sourceDatabase: 'src',
      },
    });
    expect(window.open).toHaveBeenLastCalledWith(
      '/window.html?window=data-sync&sourceId=pg-src&targetId=pg-tgt&sourceDatabase=src',
      '_blank',
      expect.any(String),
    );
    expect(mockInvoke).not.toHaveBeenCalled();
  });

  it('openSingletonWindow without params opens the bare entry', () => {
    openSingletonWindow('backup-singleton', {});
    expect(window.open).toHaveBeenLastCalledWith('/window.html', '_blank', expect.any(String));
  });

  it('openMultiInstanceWindow always opens a browser tab', () => {
    openMultiInstanceWindow('conn-1', { params: { window: 'connection' } });
    expect(window.open).toHaveBeenLastCalledWith(
      '/window.html?window=connection',
      '_blank',
      expect.any(String),
    );
  });

  it('hasOpenChildWindows answers false without touching the runtime', async () => {
    await expect(hasOpenChildWindows()).resolves.toBe(false);
  });
});

describe('desktop windowRouter — Tauri rail', () => {
  beforeEach(() => {
    envState.desktop = true;
    vi.clearAllMocks();
    mockInvoke.mockResolvedValue(undefined);
    mockGetByLabel.mockResolvedValue(null);
  });

  afterEach(() => {
    envState.desktop = false;
  });

  it('openSingletonWindow creates the sub-window through the host command', async () => {
    openSingletonWindow('data-sync-singleton', {
      params: { window: 'data-sync' },
      width: 1000,
      height: 700,
      title: 'Data Sync',
    });
    await vi.waitFor(() => expect(mockInvoke).toHaveBeenCalled());
    expect(mockInvoke).toHaveBeenCalledWith(
      'create_sub_window',
      expect.objectContaining({
        options: expect.objectContaining({
          label: 'data-sync-singleton',
          url: 'window.html?window=data-sync',
          title: 'Data Sync',
          width: 1000,
          height: 700,
          center: true,
        }),
      }),
    );
  });

  it('focuses an existing singleton instead of creating a duplicate window', async () => {
    mockGetByLabel.mockResolvedValue({
      show: mockShow,
      unminimize: mockUnminimize,
      setFocus: mockSetFocus,
    });
    openSingletonWindow('data-sync-singleton', { params: { window: 'data-sync' } });
    await vi.waitFor(() => expect(mockSetFocus).toHaveBeenCalled());
    expect(mockShow).toHaveBeenCalled();
    expect(mockInvoke).not.toHaveBeenCalledWith('create_sub_window', expect.anything());
  });

  it('[tester] focuses existing singleton when duplicate open is requested while inflight', async () => {
    let resolveInvoke!: () => void;
    mockInvoke.mockImplementation(
      () =>
        new Promise<void>((resolve) => {
          resolveInvoke = resolve;
        }),
    );
    mockGetByLabel.mockResolvedValueOnce(null).mockResolvedValue({
      show: mockShow,
      unminimize: mockUnminimize,
      setFocus: mockSetFocus,
    });
    openSingletonWindow('data-sync-singleton', { params: { window: 'data-sync' } });
    openSingletonWindow('data-sync-singleton', { params: { window: 'data-sync' } });
    await vi.waitFor(() => expect(mockSetFocus).toHaveBeenCalled());
    expect(mockShow).toHaveBeenCalled();
    expect(mockUnminimize).toHaveBeenCalled();
    resolveInvoke();
  });

  it('shows error dialog when invoke fails for other singleton windows', async () => {
    mockInvoke.mockRejectedValue(new Error('permission denied'));
    const errSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
    openSingletonWindow('data-sync-singleton', { params: { window: 'data-sync' } });
    await vi.waitFor(() => expect(mockMessage).toHaveBeenCalled());
    expect(mockMessage).toHaveBeenCalledWith(
      expect.stringContaining('permission denied'),
      expect.objectContaining({ kind: 'error' }),
    );
    errSpy.mockRestore();
  });

  it('opens distinct singletons without blocking each other', async () => {
    openSingletonWindow('data-sync-singleton', { params: { window: 'data-sync' } });
    openSingletonWindow('data-transfer-singleton', { params: { window: 'data-transfer' } });
    openSingletonWindow('schema-diff-singleton', { params: { window: 'schema-diff' } });
    await vi.waitFor(() => expect(mockInvoke).toHaveBeenCalledTimes(3));
    const labels = mockInvoke.mock.calls.map(
      (call) => (call[1] as { options: { label: string } }).options.label,
    );
    expect(labels).toEqual(
      expect.arrayContaining([
        'data-sync-singleton',
        'data-transfer-singleton',
        'schema-diff-singleton',
      ]),
    );
  });

  it('hasOpenChildWindows asks the runtime for non-main windows', async () => {
    const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');
    vi.mocked(WebviewWindow.getAll).mockResolvedValue([
      { label: 'main' },
      { label: 'backup-singleton' },
    ] as never[]);
    await expect(hasOpenChildWindows()).resolves.toBe(true);
    vi.mocked(WebviewWindow.getAll).mockResolvedValue([{ label: 'main' }] as never[]);
    await expect(hasOpenChildWindows()).resolves.toBe(false);
  });
});
