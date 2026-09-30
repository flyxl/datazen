/**
 * SQL Editor Enhanced extension point — privileged editor capabilities
 * (intentions, hover, signature help, JOIN completion, paste-as-IN, drop caret, linter).
 *
 * Host ships a no-op fallback; enhanced extensions register the implementation.
 */
import type { Extension } from '@codemirror/state';
import type { KeyBinding } from '@codemirror/view';
import type { CompletionSource } from '@codemirror/autocomplete';
import type React from 'react';
import { createExtensionPoint } from './extensionPoints';
import type { QueryBuilderContribution } from './queryBuilder';
import type { SqlParamDialectPolicy } from './sql-editor/bindParams';

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

/**
 * A privileged extension's contribution to a host-owned React panel slot.
 *
 * The host owns the frame (placement, styling, lifecycle); the extension owns
 * only what goes inside it, plus an optional teardown hook.
 */
export interface EditorPanelSlot {
  /** Render the panel body. */
  render: () => React.ReactNode;
  /** Teardown for subscriptions/timers owned by the panel. */
  dispose?: () => void;
}

export interface SqlEditorEnhancedFeatures {
  /** Pro-only visual SQL query builder UI and lifecycle. */
  queryBuilder?: QueryBuilderContribution;
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
  useBindParameters?: (sql: string, policy?: SqlParamDialectPolicy) => any;
  /**
   * S4-E: as-you-type static diagnostics (unknown table / column squiggles).
   * Returns `@codemirror/lint` extensions; debounced and degraded on large documents.
   */
  createLinterExtensions?: (
    opts: SqlEditorEnhancedOptions,
    refs: SqlEditorEnhancedOptions,
  ) => Extension[];
  /**
   * Generic escape hatch for capabilities the host has no dedicated slot for.
   *
   * The host installs the result in its own re-configurable `extra` compartment,
   * so the extension never needs a host release to ship a new editor
   * capability. Returns `[]` when nothing applies.
   */
  createExtraExtensions?: (opts?: SqlEditorEnhancedOptions) => Extension[];
  /**
   * Code folding: fold state, fold gutter and fold keymap.
   *
   * The host installs the result in a dedicated `fold` compartment rather than
   * in the generic `extra` bucket, because folding carries a keymap. It needs
   * a known position in the mount order — which is the precedence order — and
   * it has to be reconfigurable on its own, without rewriting the generic
   * bucket's contents in the same transaction.
   *
   * Optional and absent-means-empty, so adding it required no
   * `EXTENSION_POINTS_VERSION` bump: that guard is exact string equality
   * (`security.ts`), and bumping it would reject every EP whose manifest still
   * declares 1.1.0. Both directions of version drift degrade rather than
   * break — an extension this new running against an older host has its
   * extensions folded into `extra` by the host's overflow path.
   */
  createFoldExtensions?: (opts?: SqlEditorEnhancedOptions) => Extension[];
  /**
   * Extra keymap bindings.
   *
   * The host installs them with `Prec.highest`, which is what makes this hook
   * useful: `createBaseEditorExtensions` registers `defaultKeymap` *before* any
   * Pro compartment, and several of its commands return `true` whenever they
   * do anything at all — `copyLineUp`/`copyLineDown`
   * (`Shift-Alt-ArrowUp`/`Down`), `moveLineUp`/`moveLineDown`
   * (`Alt-ArrowUp`/`Down`), `addCursorAbove`/`addCursorBelow`
   * (`Mod-Alt-ArrowUp`/`Down`). CodeMirror stops at the first handler that
   * returns `true`, so at default precedence a binding on one of those chords
   * would be silently swallowed and the keystroke would never reach the
   * extension. Returning `false` from a handler hands the chord back to the
   * host binding, so `Prec.highest` widens reach without making the hook greedy.
   */
  createExtraKeymap?: (opts?: SqlEditorEnhancedOptions) => KeyBinding[];
  /**
   * Generic React panel slot. Returns `null` when the requested `slotId` is not
   * one this extension provides.
   */
  createEditorPanelSlot?: (slotId: string, ctx: SqlEditorEnhancedOptions) => EditorPanelSlot | null;
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
  createExtraExtensions: () => [],
  createExtraKeymap: () => [],
  createEditorPanelSlot: () => null,
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
    'Privileged SQL editor capabilities: visual query builder, intentions, hover, signature help, JOIN completion, paste-as-IN, drop caret, linter.',
  getDefault: () => fallbackFeatures,
});

/** @deprecated Use `sqlEditorEnhancedEP` instead. */
export const sqlEditorProEP = sqlEditorEnhancedEP;
