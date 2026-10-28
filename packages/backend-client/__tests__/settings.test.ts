import { describe, expect, it } from 'vitest';
import {
  createBackendClient,
  toSharedSettings,
  type BackendTransport,
  type MethodMap,
} from '../src';

const source = {
  theme: { mode: 'dark' as const, packId: null },
  language: 'en',
  editorFontSize: 13,
  editorFontFamily: 'Menlo',
  confirmOnDelete: true,
  autoCommit: true,
  confirmDangerousExecution: true,
  defaultPageSize: 50,
  autoChartOnQuery: false,
  wappSettings: {},
  limitSelectResults: false,
  queryResultLimit: 5000,
  safeMode: true,
  connectionPoolSize: 10,
  mcpDisabledTools: [],
  mcpPermissionMode: 'safe_write' as const,
  mcpAllowedConnectionIds: [],
  driverSettings: {},
  aiStrictEgress: true,
  logPath: '/native/private',
  // Form-split F4: monitor / onboarding / rowCountMode are shared-contract
  // fields now, so they ride the projection instead of being stripped. The
  // email password inside monitor is *alerting configuration*, not a host
  // credential — the §7.1 whitelist review (plan §5.4 field-ownership pass)
  // keeps host secrets (paths, keys) out but carries the behavior fields.
  monitor: {
    trayEnabled: true,
    closeToTray: false,
    email: { enabled: false, host: 'smtp.example.test', port: 587 },
    maxConcurrentQueries: 2,
    exportIncludeDashboardRuns: true,
    runRetentionCount: 200,
    runRetentionDays: 30,
  },
  onboarding: { completed: true, version: 1 },
  rowCountMode: 'auto' as const,
};
describe('portable settings boundary', () => {
  it('projects a whitelist and fills missing legacy preferences', () => {
    const snapshot = toSharedSettings(source);
    expect(JSON.stringify(snapshot)).not.toContain('/native/private');
    expect(snapshot.preferences.sqlSnippets).toEqual([]);
    expect(snapshot.preferences.workflowStepResultOrder).toBe('desc');
    expect(snapshot.preferences.sqlFormatOptions?.keywordCase).toBe('upper');
    // Form-split F4: the three behavior fields survive the projection —
    // they are shared-contract fields, not host topology.
    expect(snapshot.monitor.runRetentionDays).toBe(30);
    expect(snapshot.onboarding).toEqual({ completed: true, version: 1 });
    expect(snapshot.rowCountMode).toBe('auto');
  });
  it('routes shared reads and writes through the selected backend facade', async () => {
    const snapshot = toSharedSettings(source);
    const calls: Array<{ method: keyof MethodMap; payload: unknown }> = [];
    const transport: BackendTransport = {
      async call<K extends keyof MethodMap>(method: K, payload: MethodMap[K]['request']) {
        calls.push({ method, payload });
        return snapshot as MethodMap[K]['response'];
      },
    };
    const client = createBackendClient('remote', transport);
    expect(await client.getSettings()).toEqual(snapshot);
    expect(await client.saveSettings(snapshot)).toEqual(snapshot);
    expect(calls).toEqual([
      { method: 'getSettings', payload: undefined },
      { method: 'saveSettings', payload: { settings: snapshot } },
    ]);
  });
});
