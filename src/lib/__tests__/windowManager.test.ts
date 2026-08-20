import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const mockInvoke = vi.fn().mockResolvedValue(undefined);
const mockMessage = vi.fn().mockResolvedValue(undefined);
const mockEmitCrossWindow = vi.fn().mockResolvedValue(undefined);
const mockShow = vi.fn().mockResolvedValue(undefined);
const mockUnminimize = vi.fn().mockResolvedValue(undefined);
const mockSetFocus = vi.fn().mockResolvedValue(undefined);

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({
  message: (...args: unknown[]) => mockMessage(...args),
}));

vi.mock('../crossWindowBus', () => ({
  emitCrossWindow: (...args: unknown[]) => mockEmitCrossWindow(...args),
}));

vi.mock('@tauri-apps/api/webviewWindow', () => ({
  WebviewWindow: {
    getByLabel: vi.fn().mockResolvedValue({
      show: mockShow,
      unminimize: mockUnminimize,
      setFocus: mockSetFocus,
    }),
  },
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

  it('exports capability label samples without settings sub-window', async () => {
    const { WINDOW_CAPABILITY_LABEL_SAMPLES } = await import('../windowManager');
    expect(WINDOW_CAPABILITY_LABEL_SAMPLES).toContain('main');
    expect(WINDOW_CAPABILITY_LABEL_SAMPLES).not.toContain('settings-singleton');
  });

  it('openSettingsWindow emits menu:open-settings with section in browser mode', async () => {
    const { openSettingsWindow } = await import('../windowManager');
    openSettingsWindow('ai');
    await vi.waitFor(() => expect(mockEmitCrossWindow).toHaveBeenCalled());
    expect(mockEmitCrossWindow).toHaveBeenCalledWith('menu:open-settings', { section: 'ai' });
    expect(window.open).not.toHaveBeenCalled();
  });

  it('openSettingsWindow emits menu:open-settings without payload when no section', async () => {
    const { openSettingsWindow } = await import('../windowManager');
    openSettingsWindow();
    await vi.waitFor(() => expect(mockEmitCrossWindow).toHaveBeenCalled());
    expect(mockEmitCrossWindow).toHaveBeenCalledWith('menu:open-settings', undefined);
    expect(window.open).not.toHaveBeenCalled();
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

  it('openSettingsWindow focuses main and emits menu:open-settings instead of create_sub_window', async () => {
    vi.resetModules();
    const { openSettingsWindow } = await import('../windowManager');
    openSettingsWindow('logging');
    await vi.waitFor(() => expect(mockEmitCrossWindow).toHaveBeenCalled());
    expect(mockEmitCrossWindow).toHaveBeenCalledWith('menu:open-settings', { section: 'logging' });
    expect(mockShow).toHaveBeenCalled();
    expect(mockSetFocus).toHaveBeenCalled();
    expect(mockInvoke).not.toHaveBeenCalled();
  });

  it('shows error dialog when invoke fails for other singleton windows', async () => {
    vi.resetModules();
    mockInvoke.mockRejectedValue(new Error('permission denied'));
    const errSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
    const { openDataSyncWindow } = await import('../windowManager');
    openDataSyncWindow();
    await vi.waitFor(() => expect(mockMessage).toHaveBeenCalled());
    expect(mockMessage).toHaveBeenCalledWith(
      expect.stringContaining('permission denied'),
      expect.objectContaining({ kind: 'error' }),
    );
    errSpy.mockRestore();
  });
});
