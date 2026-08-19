import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const mockInvoke = vi.fn().mockResolvedValue(undefined);
const mockMessage = vi.fn().mockResolvedValue(undefined);

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({
  message: (...args: unknown[]) => mockMessage(...args),
}));

describe('windowManager — browser', () => {
  beforeEach(() => {
    delete (window as Record<string, unknown>).__TAURI_INTERNALS__;
    vi.clearAllMocks();
    vi.spyOn(window, 'open').mockReturnValue(null);
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('exports capability label samples', async () => {
    const { WINDOW_CAPABILITY_LABEL_SAMPLES } = await import('../windowManager');
    expect(WINDOW_CAPABILITY_LABEL_SAMPLES).toContain('main');
    expect(WINDOW_CAPABILITY_LABEL_SAMPLES).toContain('settings-singleton');
  });

  it('openSettingsWindow opens browser tab with params', async () => {
    const { openSettingsWindow } = await import('../windowManager');
    openSettingsWindow('ai');
    expect(window.open).toHaveBeenCalledWith(
      '/window.html?window=settings&section=ai',
      '_blank',
      expect.stringContaining('width=720'),
    );
  });

  it('openNewConnectionWindow with editId', async () => {
    const { openNewConnectionWindow } = await import('../windowManager');
    openNewConnectionWindow('cfg-1');
    expect(window.open).toHaveBeenCalledWith(
      '/window.html?window=new-connection&editId=cfg-1',
      '_blank',
      expect.any(String),
    );
  });

  it('openConnectionWindow stores pending connection and focuses main workspace', async () => {
    const { openConnectionWindow, PENDING_CONNECTION_KEY } = await import('../windowManager');
    openConnectionWindow({ connectionId: 'c1', configId: 'cfg1' }, 'My DB', 'app', 'postgresql');
    expect(window.open).not.toHaveBeenCalled();
    const pending = JSON.parse(localStorage.getItem(PENDING_CONNECTION_KEY) ?? '{}');
    expect(pending.configId).toBe('cfg1');
    expect(pending.connectionId).toBe('c1');
    expect(pending.database).toBe('app');
  });

  it('openDataSyncWindow, backup, workflow, docs, dashboard', async () => {
    const {
      openDataSyncWindow,
      openBackupWindow,
      openWorkflowWindow,
      openDocsWindow,
      openDashboardWindow,
    } = await import('../windowManager');

    openDataSyncWindow();
    expect(window.open).toHaveBeenLastCalledWith(
      '/window.html?window=data-sync',
      '_blank',
      expect.any(String),
    );

    openBackupWindow();
    expect(window.open).toHaveBeenLastCalledWith(
      '/window.html?window=backup',
      '_blank',
      expect.any(String),
    );

    openBackupWindow('restore');
    expect(window.open).toHaveBeenLastCalledWith(
      '/window.html?window=backup&mode=restore',
      '_blank',
      expect.any(String),
    );

    const openCallsBeforeWorkflow = vi.mocked(window.open).mock.calls.length;
    openWorkflowWindow();
    expect(vi.mocked(window.open).mock.calls.length).toBe(openCallsBeforeWorkflow);

    openDocsWindow('getting-started');
    expect(window.open).toHaveBeenLastCalledWith(
      '/window.html?window=docs&section=getting-started',
      '_blank',
      expect.any(String),
    );

    const openCallsBeforeDashboard = vi.mocked(window.open).mock.calls.length;
    openDashboardWindow('dash-1', 'Sales');
    expect(vi.mocked(window.open).mock.calls.length).toBe(openCallsBeforeDashboard);
  });
});

describe('windowManager — Tauri', () => {
  beforeEach(() => {
    (window as Record<string, unknown>).__TAURI_INTERNALS__ = {};
    vi.clearAllMocks();
    mockInvoke.mockResolvedValue(undefined);
  });

  afterEach(() => {
    delete (window as Record<string, unknown>).__TAURI_INTERNALS__;
  });

  it('openSettingsWindow invokes create_sub_window', async () => {
    vi.resetModules();
    const { openSettingsWindow } = await import('../windowManager');
    openSettingsWindow();
    await vi.waitFor(() => expect(mockInvoke).toHaveBeenCalled());
    expect(mockInvoke).toHaveBeenCalledWith('create_sub_window', {
      options: expect.objectContaining({
        label: 'settings-singleton',
        url: 'window.html?window=settings',
      }),
    });
  });

  it('shows error dialog when invoke fails', async () => {
    vi.resetModules();
    mockInvoke.mockRejectedValue(new Error('permission denied'));
    const errSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
    const { openSettingsWindow } = await import('../windowManager');
    openSettingsWindow();
    await vi.waitFor(() => expect(mockMessage).toHaveBeenCalled());
    expect(mockMessage).toHaveBeenCalledWith(
      expect.stringContaining('permission denied'),
      expect.objectContaining({ kind: 'error' }),
    );
    errSpy.mockRestore();
  });
});
