import { describe, it, expect } from 'vitest';
import { exportEditorSettings, importEditorSettings } from '../settingsExport';
import type { AppSettings } from '../../types';

describe('settingsExport', () => {
  const mockSettings: AppSettings = {
    theme: { mode: 'dark', packId: null },
    language: 'en',
    limitSelectResults: true,
    queryResultLimit: 5000,
    editorFontSize: 14,
    editorFontFamily: 'Fira Code',
    confirmOnDelete: true,
    autoCommit: true,
    safeMode: true,
    confirmDangerousExecution: true,
    defaultPageSize: 50,
    connectionPoolSize: 10,
    logLevel: 'info',
    logPath: '',
    mcpServerEnabled: false,
    mcpDisabledTools: [],
    mcpPermissionMode: 'safe_write',
    mcpAllowedConnectionIds: [],
    contextDir: '',
    checkForUpdatesOnStartup: false,
    autoChartOnQuery: false,
    monitor: {
      trayEnabled: false,
      closeToTray: false,
      maxConcurrentQueries: 2,
      exportIncludeDashboardRuns: false,
      runRetentionCount: 50,
      runRetentionDays: 30,
    },
    driverSettings: {},
    wappSettings: {},
    aiStrictEgress: true,
    editorCompletionQuotePolicy: 'always',
    keymapPreset: 'dbeaver',
    customKeymap: { 'editor.execute': 'ctrl-enter' },
    sqlExecutionStrategy: 'entire_script',
    sqlFormatOptions: {
      keywordCase: 'upper',
      indentStyle: '2spaces',
      breakBeforeBooleanOperators: true,
      linesBetweenQueries: 2,
    },
    sqlSnippets: [
      {
        id: 'user-s1',
        prefix: 'selc',
        descriptionKey: 'Select count',
        template: 'SELECT COUNT(*) FROM #{1:table};',
      },
    ],
    rowCountMode: 'auto',
    rowCountEstimateThreshold: 200000,
    defaultTableSort: 'primaryKeyAsc',
    gridDensity: 'standard',
    gridAlternatingRows: true,
    gridRowNumbers: true,
    gridAutoHideScrollbar: false,
  };

  it('exports only whitelisted safe editor and snippet properties', () => {
    const jsonStr = exportEditorSettings(mockSettings);
    const parsed = JSON.parse(jsonStr);

    expect(parsed.version).toBe(1);
    expect(parsed.editor.fontSize).toBe(14);
    expect(parsed.editor.fontFamily).toBe('Fira Code');
    expect(parsed.editor.sqlExecutionStrategy).toBe('entire_script');
    expect(parsed.sqlSnippets).toHaveLength(1);
    expect(parsed.sqlSnippets[0].prefix).toBe('selc');

    // Sensitive settings must NEVER be present
    expect(parsed.theme).toBeUndefined();
    expect(parsed.mcpAllowedConnectionIds).toBeUndefined();
    expect(parsed.driverSettings).toBeUndefined();
    expect(parsed.wappSettings).toBeUndefined();
  });

  it('imports valid exported json correctly', () => {
    const jsonStr = exportEditorSettings(mockSettings);
    const imported = importEditorSettings(jsonStr);

    expect(imported.editorFontSize).toBe(14);
    expect(imported.editorFontFamily).toBe('Fira Code');
    expect(imported.sqlExecutionStrategy).toBe('entire_script');
    expect(imported.sqlSnippets).toHaveLength(1);
    expect(imported.sqlSnippets?.[0].prefix).toBe('selc');
  });

  it.each([true, false])('round-trips the table prefix preference: %s', (enabled) => {
    const exported = exportEditorSettings({
      ...mockSettings,
      editorCompletionIncludeTablePrefix: enabled,
    });
    expect(JSON.parse(exported).editor.completionIncludeTablePrefix).toBe(enabled);
    expect(importEditorSettings(exported).editorCompletionIncludeTablePrefix).toBe(enabled);
  });

  it('leaves the current preference unchanged when importing older settings', () => {
    const imported = importEditorSettings(exportEditorSettings(mockSettings));
    expect(imported).not.toHaveProperty('editorCompletionIncludeTablePrefix');
  });

  it.each(['false', 0, null, {}])('ignores invalid table prefix preferences: %j', (value) => {
    const imported = importEditorSettings(
      JSON.stringify({
        editor: { completionIncludeTablePrefix: value },
      }),
    );
    expect(imported).not.toHaveProperty('editorCompletionIncludeTablePrefix');
  });

  it('rejects invalid json', () => {
    expect(() => importEditorSettings('invalid json')).toThrow('Invalid JSON format');
    expect(() => importEditorSettings('123')).toThrow('Root must be an object');
  });
});
