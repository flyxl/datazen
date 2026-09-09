/**
 * SQL Editor Enhanced extension point — privileged editor capabilities
 * (intentions, hover, signature help, JOIN completion, paste-as-IN, drop caret, linter).
 *
 * Host ships a no-op fallback; enhanced extensions register the implementation.
 */
import type { Extension } from '@codemirror/state';
import type { CompletionSource } from '@codemirror/autocomplete';
import type React from 'react';
import { createExtensionPoint } from './extensionPoints';

/** Compartment option bags passed from host; typed loosely for cross-repo linking. */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export type SqlEditorEnhancedOptions = Record<string, any>;

/** @deprecated Use `SqlEditorEnhancedOptions` instead. */
export type SqlEditorProOptions = SqlEditorEnhancedOptions;

export interface ExtensionSettingOption {
  label: string;
  value: string | number;
}

export interface ExtensionSettingRenderProps<T = unknown> {
  value: T;
  onChange: (newValue: T) => void;
  values: Record<string, unknown>;
  updateSetting: (key: string, val: unknown) => void;
}

export interface ExtensionGroupRenderProps {
  values: Record<string, unknown>;
  updateSetting: (key: string, val: unknown) => void;
  items?: ExtensionSettingItem[];
}

export interface ExtensionSettingItem {
  key: string;
  label: string;
  hint?: string;
  type: 'boolean' | 'number' | 'select' | 'input' | 'custom';
  defaultValue?: unknown;
  options?: ExtensionSettingOption[];
  /** When type is 'custom', called to render custom UI component. */
  render?: (props: ExtensionSettingRenderProps) => React.ReactNode;
}

export interface ExtensionSettingsContribution {
  extensionId: string;
  targetSection: 'editor' | 'general' | 'appearance';
  groupTitle?: string;
  groupDescription?: string;
  items?: ExtensionSettingItem[];
  /** Custom renderer that replaces the entire settings group card. */
  renderGroup?: (props: ExtensionGroupRenderProps) => React.ReactNode;
}

export interface SqlEditorEnhancedFeatures {
  /** S4-A: statement frame + gutter run button extensions. */
  createStatementDecorations?: (opts?: SqlEditorEnhancedOptions) => Extension[];
  /** S4-C: Alt+Enter intentions + INSERT/function inlay hints. */
  createIntentionExtensions?: (
    opts: SqlEditorEnhancedOptions,
    refs: SqlEditorEnhancedOptions,
  ) => Extension[];
  /** S4-D: table hover tooltip + Mod/Cmd+Click navigation. */
  createHoverExtensions?: (
    opts: SqlEditorEnhancedOptions,
    refs: SqlEditorEnhancedOptions,
  ) => Extension[];
  /** S4-B: function signature help tooltip. */
  createSignatureHelpExtensions?: (databaseType?: string) => Extension[];
  /** S4-B: FK-aware JOIN completion source (returns null when not applicable). */
  createJoinCompletionSource?: (
    opts: SqlEditorEnhancedOptions,
    refs: SqlEditorEnhancedOptions,
  ) => CompletionSource | null;
  /** Enhanced column completion source (intelligent disambiguation & auto-FROM). */
  createColumnCompletionSource?: (
    opts: SqlEditorEnhancedOptions,
    refs: SqlEditorEnhancedOptions,
  ) => CompletionSource | null;
  /** S5-A: paste-as-IN keymap + schema-tree drop caret. */
  createPasteExtensions?: (opts: SqlEditorEnhancedOptions) => Extension[];
  /** S5-A: context-menu group for paste-as-IN (null when unavailable). */
  createPasteAsInContextMenuItems?: () => SqlEditorEnhancedOptions | null;
  /** S5-B: Bind parameter panel renderer. */
  renderBindParamPanel?: (props: SqlEditorEnhancedOptions) => any;
  /** S5-B: Bind parameters hook. */
  useBindParameters?: (sql: string, options?: any) => any;
  /**
   * S4-E: as-you-type static diagnostics (unknown table / column squiggles).
   * Returns `@codemirror/lint` extensions; debounced and degraded on large documents.
   */
  createLinterExtensions?: (
    opts: SqlEditorEnhancedOptions,
    refs: SqlEditorEnhancedOptions,
  ) => Extension[];
  /** Settings contributions. */
  settingsContributions?: ExtensionSettingsContribution[];
}

/** @deprecated Use `SqlEditorEnhancedFeatures` instead. */
export type SqlEditorProFeatures = SqlEditorEnhancedFeatures;

const fallbackFeatures: SqlEditorEnhancedFeatures = Object.freeze({
  createStatementDecorations: () => [],
  createIntentionExtensions: () => [],
  createHoverExtensions: () => [],
  createSignatureHelpExtensions: () => [],
  createJoinCompletionSource: () => null,
  createColumnCompletionSource: () => null,
  createPasteExtensions: () => [],
  createPasteAsInContextMenuItems: () => null,
  createLinterExtensions: () => [],
  renderBindParamPanel: () => null,
  useBindParameters: () => ({
    params: [],
    values: {},
    labels: {},
    activeParamIds: [],
    paramHistory: {},
    setValue: () => {},
    applyHistoryEntry: () => {},
    clearHistory: () => {},
    markSubmitted: () => {},
    getHistory: () => [],
  }),
  settingsContributions: [],
});

export const sqlEditorEnhancedEP = createExtensionPoint<SqlEditorEnhancedFeatures>({
  id: 'editor.sql.enhanced',
  name: 'SQL Editor Enhanced',
  description:
    'Privileged SQL editor capabilities: intentions, hover, signature help, JOIN completion, paste-as-IN, drop caret, linter.',
  getDefault: () => fallbackFeatures,
});

/** @deprecated Use `sqlEditorEnhancedEP` instead. */
export const sqlEditorProEP = sqlEditorEnhancedEP;
