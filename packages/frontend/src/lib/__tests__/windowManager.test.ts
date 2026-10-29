import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { DOCS_BASE_EN, DOCS_BASE_ZH } from '../docsUrls';

/**
 * Shared window-surface tests (form-split F6 prep).
 *
 * The high-level helpers ride the `WindowRouter` port and the cross-window
 * bus — this suite binds a spy platform and pins the contract: labels,
 * params, sizes, event payloads. The low-level machinery (create_sub_window,
 * WebviewWindow focus, the browser-tab fallback) is app-owned and covered by
 * `src/platform/__tests__/windowRouter.desktop.test.ts`.
 */

const mockOpenPath = vi.fn().mockResolvedValue(undefined);
const mockEmitCrossWindow = vi.fn().mockResolvedValue(undefined);
const spyOpenSingleton = vi.fn();
const spyFocusMainWindow = vi.fn().mockResolvedValue(undefined);
const spyHasOpenChildWindows = vi.fn().mockResolvedValue(false);

// The form branch is the compile-time IS_DESKTOP_BUILD constant (shared
// `platform/env`, pure import.meta.env); a getter-export mock lets each
// describe flip the form.
const envState = vi.hoisted(() => ({ desktop: false }));
vi.mock('../../platform/env', () => ({
  get IS_DESKTOP_BUILD() {
    return envState.desktop;
  },
}));

vi.mock('../../commands/settings', () => ({
  settingsCommands: {
    openPath: (...args: unknown[]) => mockOpenPath(...args),
  },
}));

vi.mock('../crossWindowBus', () => ({
  emitCrossWindow: (...args: unknown[]) => mockEmitCrossWindow(...args),
}));

vi.mock('../connectionEditor', () => ({
  openNewConnectionDialog: vi.fn(),
}));

vi.mock('../../stores/settingsStore', () => ({
  useSettingsStore: {
    getState: () => ({ settings: { language: 'en' } }),
  },
}));

import { clearPlatform, configurePlatform, testPlatform } from '../../test/setup';
import {
  WINDOW_CAPABILITY_LABEL_SAMPLES,
  hasOpenChildWindows,
  openBackupWindow,
  openConnectionWindow,
  openDashboardWindow,
  openDataSyncWindow,
  openDataTransferWindow,
  openDocsWindow,
  openNewConnectionDialog,
  openSchemaDiffWindow,
  openSettingsWindow,
  openWorkflowWindow,
  PENDING_CONNECTION_KEY,
  POST_ONBOARDING_SAMPLE_KEY,
  emitMenuAction,
} from '../windowManager';
import { openNewConnectionDialog as editorMock } from '../connectionEditor';

function bindSpyPlatform() {
  configurePlatform({
    ...testPlatform,
    windows: {
      ...testPlatform.windows,
      openSingletonWindow: spyOpenSingleton,
      focusMainWindow: spyFocusMainWindow,
      hasOpenChildWindows: spyHasOpenChildWindows,
    },
    events: {
      emitCrossWindow: (...args: unknown[]) => mockEmitCrossWindow(...args),
      listenCrossWindow: async () => () => undefined,
    },
  });
}

describe('windowManager — shared surface on the WindowRouter port', () => {
  beforeEach(() => {
    envState.desktop = false;
    vi.clearAllMocks();
    bindSpyPlatform();
  });

  afterEach(() => {
    clearPlatform();
  });

  it('exports capability label samples without new-connection sub-window', () => {
    expect(WINDOW_CAPABILITY_LABEL_SAMPLES).toContain('main');
    expect(WINDOW_CAPABILITY_LABEL_SAMPLES).not.toContain('new-connection-singleton');
    expect(WINDOW_CAPABILITY_LABEL_SAMPLES).not.toContain('settings-singleton');
    expect(WINDOW_CAPABILITY_LABEL_SAMPLES).not.toContain('docs-singleton');
  });

  it('openSettingsWindow emits menu:open-settings with section in browser mode', async () => {
    openSettingsWindow('ai');
    await vi.waitFor(() => expect(mockEmitCrossWindow).toHaveBeenCalled());
    expect(mockEmitCrossWindow).toHaveBeenCalledWith('menu:open-settings', { section: 'ai' });
    expect(spyOpenSingleton).not.toHaveBeenCalled();
  });

  it('openSettingsWindow emits menu:open-settings without payload when no section', async () => {
    openSettingsWindow();
    await vi.waitFor(() => expect(mockEmitCrossWindow).toHaveBeenCalled());
    expect(mockEmitCrossWindow).toHaveBeenCalledWith('menu:open-settings', undefined);
  });

  it('openNewConnectionDialog opens the in-app editor and focuses the main window', async () => {
    openNewConnectionDialog('cfg-1');
    await vi.waitFor(() => expect(editorMock).toHaveBeenCalledWith('cfg-1', undefined));
    expect(spyFocusMainWindow).toHaveBeenCalled();
    expect(spyOpenSingleton).not.toHaveBeenCalled();
  });

  it('openConnectionWindow stores pending connection and focuses main workspace', () => {
    openConnectionWindow({ dbSessionId: 'c1', connectionId: 'cfg1' }, 'My DB', 'app', 'postgresql');
    expect(spyOpenSingleton).not.toHaveBeenCalled();
    const pending = JSON.parse(localStorage.getItem(PENDING_CONNECTION_KEY) ?? '{}');
    expect(pending.connectionId).toBe('cfg1');
    expect(pending.dbSessionId).toBe('c1');
    expect(pending.database).toBe('app');
    expect(mockEmitCrossWindow).toHaveBeenCalledWith(
      'datazen:open-connection',
      expect.objectContaining({
        connectionName: 'My DB',
        connectionId: 'cfg1',
      }),
    );
  });

  it('serializes migration prefill params into the router options', () => {
    openDataSyncWindow({ sourceId: 'pg-src', targetId: 'pg-tgt', sourceDatabase: 'src' });
    expect(spyOpenSingleton).toHaveBeenCalledWith('data-sync-singleton', {
      params: {
        window: 'data-sync',
        sourceId: 'pg-src',
        targetId: 'pg-tgt',
        sourceDatabase: 'src',
      },
      width: 1000,
      height: 700,
      minWidth: 600,
      minHeight: 480,
      title: expect.any(String),
    });

    openDataTransferWindow({ sourceId: 'a', targetId: 'b' });
    expect(spyOpenSingleton).toHaveBeenCalledWith(
      'data-transfer-singleton',
      expect.objectContaining({
        params: { window: 'data-transfer', sourceId: 'a', targetId: 'b' },
      }),
    );

    openSchemaDiffWindow({ sourceSchema: 'public', targetSchema: 'app' });
    expect(spyOpenSingleton).toHaveBeenCalledWith(
      'schema-diff-singleton',
      expect.objectContaining({
        params: { window: 'schema-diff', sourceSchema: 'public', targetSchema: 'app' },
      }),
    );
  });

  it('openBackupWindow routes restore mode to the restore singleton with the mode param', () => {
    openBackupWindow();
    expect(spyOpenSingleton).toHaveBeenCalledWith(
      'backup-singleton',
      expect.objectContaining({ params: { window: 'backup' } }),
    );

    openBackupWindow('restore');
    expect(spyOpenSingleton).toHaveBeenCalledWith(
      'backup-restore-singleton',
      expect.objectContaining({ params: { window: 'backup', mode: 'restore' } }),
    );
  });

  it('openWorkflowWindow focuses main and emits menu:workflow', async () => {
    openWorkflowWindow();
    await vi.waitFor(() => expect(spyFocusMainWindow).toHaveBeenCalled());
    expect(mockEmitCrossWindow).toHaveBeenCalledWith('menu:workflow');
  });

  it('openDashboardWindow emits the dashboard menu event, not a sub-window', async () => {
    openDashboardWindow('dash-1', 'Sales');
    await vi.waitFor(() =>
      expect(mockEmitCrossWindow).toHaveBeenCalledWith('menu:open-dashboard', {
        dashboardId: 'dash-1',
        dashboardName: 'Sales',
      }),
    );
    expect(spyOpenSingleton).not.toHaveBeenCalled();
  });

  it('emitMenuAction focuses the main shell for main-shell ids before emitting', async () => {
    await emitMenuAction('open-settings', { section: 'logging' });
    expect(spyFocusMainWindow).toHaveBeenCalled();
    expect(mockEmitCrossWindow).toHaveBeenCalledWith('menu:open-settings', { section: 'logging' });

    spyFocusMainWindow.mockClear();
    await emitMenuAction('toggle-sidebar');
    // Not a main-shell id: no focus detour.
    expect(spyFocusMainWindow).not.toHaveBeenCalled();
    expect(mockEmitCrossWindow).toHaveBeenCalledWith('menu:toggle-sidebar', undefined);
  });

  it('hasOpenChildWindows answers through the port', async () => {
    spyHasOpenChildWindows.mockResolvedValueOnce(true);
    await expect(hasOpenChildWindows()).resolves.toBe(true);
    expect(spyHasOpenChildWindows).toHaveBeenCalled();
  });

  it('exports the pending-connection and post-onboarding sample keys unchanged', () => {
    expect(PENDING_CONNECTION_KEY).toBe('datazen:pending-connection');
    expect(POST_ONBOARDING_SAMPLE_KEY).toBe('datazen:post-onboarding-sample');
  });

  it('openDocsWindow opens the browser URL in a browser build', () => {
    const openSpy = vi.spyOn(window, 'open').mockReturnValue(null);
    openDocsWindow('workflows');
    expect(openSpy).toHaveBeenLastCalledWith(`${DOCS_BASE_EN}#workflow`, '_blank', 'noopener');
    expect(mockOpenPath).not.toHaveBeenCalled();
    openSpy.mockRestore();
  });

  it('openDocsWindow ignores unknown section hash', () => {
    const openSpy = vi.spyOn(window, 'open').mockReturnValue(null);
    openDocsWindow('getting-started');
    expect(openSpy).toHaveBeenLastCalledWith(DOCS_BASE_EN, '_blank', 'noopener');
    openSpy.mockRestore();
  });
});

describe('windowManager — openDocsWindow on the desktop build', () => {
  beforeEach(() => {
    envState.desktop = true;
    vi.clearAllMocks();
    bindSpyPlatform();
  });

  afterEach(() => {
    envState.desktop = false;
    clearPlatform();
  });

  it('uses the host openPath and does not create a sub-window', async () => {
    const openSpy = vi.spyOn(window, 'open').mockReturnValue(null);
    const settingsStore = await import('../../stores/settingsStore');
    const stateSpy = vi.spyOn(settingsStore.useSettingsStore, 'getState').mockReturnValue({
      settings: { language: 'zh-CN' },
    } as ReturnType<typeof settingsStore.useSettingsStore.getState>);

    openDocsWindow('ai');
    await vi.waitFor(() => expect(mockOpenPath).toHaveBeenCalled());
    expect(mockOpenPath).toHaveBeenCalledWith(`${DOCS_BASE_ZH}#ai`);
    expect(spyOpenSingleton).not.toHaveBeenCalled();
    expect(openSpy).not.toHaveBeenCalled();
    stateSpy.mockRestore();
    openSpy.mockRestore();
  });

  it('falls back to window.open when the host opener rejects', async () => {
    mockOpenPath.mockRejectedValueOnce(new Error('no opener'));
    const openSpy = vi.spyOn(window, 'open').mockReturnValue(null);

    openDocsWindow('workflows');
    await vi.waitFor(() => expect(openSpy).toHaveBeenCalled());
    expect(openSpy).toHaveBeenLastCalledWith(`${DOCS_BASE_EN}#workflow`, '_blank', 'noopener');
    openSpy.mockRestore();
  });
});
