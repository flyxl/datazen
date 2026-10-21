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
  monitor: { email: { password: 'synthetic-secret' } },
};
describe('portable settings boundary', () => {
  it('projects a whitelist and fills missing legacy preferences', () => {
    const snapshot = toSharedSettings(source);
    expect(JSON.stringify(snapshot)).not.toContain('synthetic-secret');
    expect(JSON.stringify(snapshot)).not.toContain('/native/private');
    expect(snapshot.preferences.sqlSnippets).toEqual([]);
    expect(snapshot.preferences.workflowStepResultOrder).toBe('desc');
    expect(snapshot.preferences.sqlFormatOptions?.keywordCase).toBe('upper');
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
