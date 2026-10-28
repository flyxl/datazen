import { describe, expect, it, vi } from 'vitest';

import { toSharedSettings, type SharedSettings } from '@datazen/backend-client';
import {
  createDesktopSettingsClient,
  pickHostExtras,
  toDesktopSettings,
  toSettings,
  type DesktopSettings,
} from '../desktop/settingsClient';
import type { AppSettings } from '../../types';
import type { DesktopHostServices } from '../contracts';
import { DEFAULT_MONITOR_SETTINGS } from '../../types/dashboard';

/**
 * The F4 settings demonstration contracts: the projection is the whole point.
 * The desktop implementation must answer the shared signature with the shared
 * shape — the host extras ride the port, never the shared wire.
 */

const HOST_BLOB: AppSettings = {
  theme: { mode: 'dark', packId: null },
  language: 'en',
  limitSelectResults: false,
  queryResultLimit: 5000,
  editorFontSize: 13,
  editorFontFamily: 'Menlo',
  confirmOnDelete: true,
  autoCommit: true,
  safeMode: true,
  confirmDangerousExecution: true,
  defaultPageSize: 50,
  connectionPoolSize: 10,
  logLevel: 'info',
  logPath: '/var/log/datazen',
  mcpServerEnabled: false,
  mcpDisabledTools: [],
  mcpPermissionMode: 'safe_write',
  mcpAllowedConnectionIds: [],
  contextDir: '/home/user/ai-context',
  favoritesRoot: undefined,
  checkForUpdatesOnStartup: true,
  autoChartOnQuery: false,
  monitor: { ...DEFAULT_MONITOR_SETTINGS, runRetentionDays: 14 },
  driverSettings: {},
  wappSettings: {},
  mcpClientServers: [],
  aiStrictEgress: true,
  editorCompletionIncludeTablePrefix: true,
  enableFkPrediction: false,
  editorCompletionQuotePolicy: 'unquoted',
  keymapPreset: 'default',
  customKeymap: {},
  sqlFormatOptions: {
    keywordCase: 'upper',
    indentStyle: '2spaces',
    breakBeforeBooleanOperators: true,
    linesBetweenQueries: 1,
  },
  sqlExecutionStrategy: 'current_statement',
  sqlSnippets: [],
  workflowStepResultOrder: 'desc',
  rowCountMode: 'estimated',
  rowCountEstimateThreshold: 1000,
  defaultTableSort: 'none',
  gridDensity: 'compact',
  gridAlternatingRows: true,
  gridRowNumbers: true,
  gridAutoHideScrollbar: false,
  onboarding: { completed: true, version: 1 },
};

describe('toSettings — the F4 projection', () => {
  it('strips every host extra and keeps the shared contract whole', () => {
    const desktop = toDesktopSettings(HOST_BLOB, toSharedSettings);
    const shared = toSettings(desktop);

    expect(JSON.stringify(shared)).not.toContain('/var/log/datazen');
    expect(JSON.stringify(shared)).not.toContain('/home/user/ai-context');
    expect('logLevel' in (shared as unknown as Record<string, unknown>)).toBe(false);
    expect('checkForUpdatesOnStartup' in (shared as unknown as Record<string, unknown>)).toBe(
      false,
    );
    expect('gridDensity' in (shared as unknown as Record<string, unknown>)).toBe(false);

    // The three behavior fields migrated in F4 stay on the shared side.
    expect(shared.monitor.runRetentionDays).toBe(14);
    expect(shared.onboarding).toEqual({ completed: true, version: 1 });
    expect(shared.rowCountMode).toBe('estimated');
    // The pre-existing shared half is untouched.
    expect(shared.preferences.language).toBe('en');
    expect(shared.execution.safeMode).toBe(true);
  });

  it('DesktopSettings extends the shared settings (superset typing)', () => {
    const desktop: DesktopSettings = toDesktopSettings(HOST_BLOB, toSharedSettings);
    const asShared: SharedSettings = desktop; // assignable by construction
    expect(asShared.preferences.theme.mode).toBe('dark');
    expect(desktop.logPath).toBe('/var/log/datazen');
    expect(desktop.gridDensity).toBe('compact');
  });
});

describe('pickHostExtras', () => {
  it('picks exactly the host-only keys', () => {
    const extras = pickHostExtras(HOST_BLOB);
    expect(Object.keys(extras).sort()).toEqual(
      [
        'logLevel',
        'logPath',
        'mcpServerEnabled',
        'mcpClientServers',
        'contextDir',
        'checkForUpdatesOnStartup',
        'rowCountEstimateThreshold',
        'defaultTableSort',
        'gridDensity',
        'gridAlternatingRows',
        'gridRowNumbers',
        'gridAutoHideScrollbar',
      ].sort(),
    );
    // The migrated behavior fields are NOT host extras anymore.
    expect(extras).not.toHaveProperty('monitor');
    expect(extras).not.toHaveProperty('onboarding');
    expect(extras).not.toHaveProperty('rowCountMode');
  });
});

describe('createDesktopSettingsClient — the HostSettingsPort adapter', () => {
  function fakeHost(): DesktopHostServices {
    return {
      getHostSettings: vi.fn().mockResolvedValue(HOST_BLOB),
      saveHostSettings: vi.fn().mockResolvedValue(undefined),
      rebuildMenu: vi.fn().mockResolvedValue(undefined),
      getLogPath: vi.fn().mockResolvedValue('/var/log/datazen'),
      getAppExecutablePath: vi.fn().mockResolvedValue('/usr/bin/datazen'),
      openPath: vi.fn().mockResolvedValue(undefined),
      openLogDir: vi.fn().mockResolvedValue(undefined),
      openWorkflowsDir: vi.fn().mockResolvedValue(undefined),
      openContextDir: vi.fn().mockResolvedValue(undefined),
      checkUpdatesOnStartup: vi.fn(),
    };
  }

  it('readHostExtras: one host read, extras-only answer', async () => {
    const host = fakeHost();
    const port = createDesktopSettingsClient(host);

    const extras = await port.readHostExtras();

    expect(host.getHostSettings).toHaveBeenCalledTimes(1);
    expect(extras.logPath).toBe('/var/log/datazen');
    expect(extras).not.toHaveProperty('theme');
    expect(extras).not.toHaveProperty('monitor');
  });

  it('writeHostSettings: delegates the full blob to the host pair', async () => {
    const host = fakeHost();
    const port = createDesktopSettingsClient(host);

    await port.writeHostSettings(HOST_BLOB);

    expect(host.saveHostSettings).toHaveBeenCalledWith(HOST_BLOB);
    expect(host.saveHostSettings).toHaveBeenCalledTimes(1);
  });

  it('routes the ancillary host services through', async () => {
    const host = fakeHost();
    const port = createDesktopSettingsClient(host);

    await port.rebuildMenu('zh-CN');
    await expect(port.getLogPath()).resolves.toBe('/var/log/datazen');
    port.checkUpdatesOnStartup(true);

    expect(host.rebuildMenu).toHaveBeenCalledWith('zh-CN');
    expect(host.checkUpdatesOnStartup).toHaveBeenCalledWith(true);
  });
});
