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
    monitor: {} as any,
    driverSettings: {},
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

  it('rejects invalid json', () => {
    expect(() => importEditorSettings('invalid json')).toThrow('Invalid JSON format');
    expect(() => importEditorSettings('123')).toThrow('Root must be an object');
  });
});
