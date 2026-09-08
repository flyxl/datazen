import type { AppSettings, SqlExecutionStrategy, SqlFormatOptions } from '../types';

export interface ExportedEditorSettings {
  version: 1;
  exportedAt: string;
  editor?: {
    fontSize?: number;
    fontFamily?: string;
    completionQuotePolicy?: 'unquoted' | 'always' | 'both';
    keymapPreset?: 'default' | 'dbeaver' | 'navicat';
    customKeymap?: Partial<Record<string, string>>;
    sqlFormatOptions?: SqlFormatOptions;
    sqlExecutionStrategy?: SqlExecutionStrategy;
  };
  sqlSnippets?: Array<{
    id: string;
    prefix: string;
    descriptionKey: string;
    template: string;
  }>;
}

/**
 * §6.4 Safe whitelist export.
 * Excludes all credentials, DB connection URLs, passwords, and AI API keys.
 */
export function exportEditorSettings(settings: AppSettings): string {
  const payload: ExportedEditorSettings = {
    version: 1,
    exportedAt: new Date().toISOString(),
    editor: {
      fontSize: settings.editorFontSize,
      fontFamily: settings.editorFontFamily,
      completionQuotePolicy: settings.editorCompletionQuotePolicy,
      keymapPreset: settings.keymapPreset,
      customKeymap: settings.customKeymap,
      sqlFormatOptions: settings.sqlFormatOptions,
      sqlExecutionStrategy: settings.sqlExecutionStrategy,
    },
    sqlSnippets: settings.sqlSnippets ?? [],
  };

  return JSON.stringify(payload, null, 2);
}

/**
 * §6.4 Validates and parses imported JSON, mapping only safe editor/snippet fields.
 */
export function importEditorSettings(jsonStr: string): Partial<AppSettings> {
  let parsed: unknown;
  try {
    parsed = JSON.parse(jsonStr);
  } catch {
    throw new Error('Invalid JSON format');
  }

  if (!parsed || typeof parsed !== 'object') {
    throw new Error('Root must be an object');
  }

  const obj = parsed as Record<string, unknown>;
  const result: Partial<AppSettings> = {};

  if (obj.editor && typeof obj.editor === 'object') {
    const ed = obj.editor as Record<string, unknown>;
    if (typeof ed.fontSize === 'number') result.editorFontSize = ed.fontSize;
    if (typeof ed.fontFamily === 'string') result.editorFontFamily = ed.fontFamily;
    if (
      ed.completionQuotePolicy === 'unquoted' ||
      ed.completionQuotePolicy === 'always' ||
      ed.completionQuotePolicy === 'both'
    ) {
      result.editorCompletionQuotePolicy = ed.completionQuotePolicy;
    }
    if (
      ed.keymapPreset === 'default' ||
      ed.keymapPreset === 'dbeaver' ||
      ed.keymapPreset === 'navicat'
    ) {
      result.keymapPreset = ed.keymapPreset;
    }
    if (ed.customKeymap && typeof ed.customKeymap === 'object') {
      result.customKeymap = ed.customKeymap as Record<string, string>;
    }
    if (ed.sqlFormatOptions && typeof ed.sqlFormatOptions === 'object') {
      result.sqlFormatOptions = ed.sqlFormatOptions as SqlFormatOptions;
    }
    if (
      ed.sqlExecutionStrategy === 'current_statement' ||
      ed.sqlExecutionStrategy === 'entire_script' ||
      ed.sqlExecutionStrategy === 'largest_statement' ||
      ed.sqlExecutionStrategy === 'ask'
    ) {
      result.sqlExecutionStrategy = ed.sqlExecutionStrategy;
    }
  }

  if (Array.isArray(obj.sqlSnippets)) {
    result.sqlSnippets = obj.sqlSnippets.filter(
      (s): s is { id: string; prefix: string; descriptionKey: string; template: string } =>
        Boolean(
          s &&
            typeof s === 'object' &&
            typeof s.id === 'string' &&
            typeof s.prefix === 'string' &&
            typeof s.template === 'string',
        ),
    );
  }

  return result;
}
