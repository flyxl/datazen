/**
 * SQL Editor Pro extension point — privileged editor enhancements
 * (intentions, hover, signature help, JOIN completion, paste-as-IN, drop caret).
 *
 * Host ships a no-op fallback; `@datazen/extension-sql-pro` registers the Pro impl.
 */
import type { Extension } from '@codemirror/state';
import type { CompletionSource } from '@codemirror/autocomplete';
import { createExtensionPoint } from './extensionPoints';

/** Compartment option bags passed from host; typed loosely for cross-repo linking. */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export type SqlEditorProOptions = Record<string, any>;

export interface SqlEditorProFeatures {
  /** S4-C: Alt+Enter intentions + INSERT/function inlay hints. */
  createIntentionExtensions?: (opts: SqlEditorProOptions, refs: SqlEditorProOptions) => Extension[];
  /** S4-D: table hover tooltip + Mod/Cmd+Click navigation. */
  createHoverExtensions?: (opts: SqlEditorProOptions, refs: SqlEditorProOptions) => Extension[];
  /** S4-B: function signature help tooltip. */
  createSignatureHelpExtensions?: (databaseType?: string) => Extension[];
  /** S4-B: FK-aware JOIN completion source (returns null when not applicable). */
  createJoinCompletionSource?: (
    opts: SqlEditorProOptions,
    refs: SqlEditorProOptions,
  ) => CompletionSource | null;
  /** S5-A: paste-as-IN keymap + schema-tree drop caret. */
  createPasteExtensions?: (opts: SqlEditorProOptions) => Extension[];
  /** S5-A: context-menu group for paste-as-IN (null when unavailable). */
  createPasteAsInContextMenuItems?: () => SqlEditorProOptions | null;
}

const fallbackFeatures: SqlEditorProFeatures = Object.freeze({
  createIntentionExtensions: () => [],
  createHoverExtensions: () => [],
  createSignatureHelpExtensions: () => [],
  createJoinCompletionSource: () => null,
  createPasteExtensions: () => [],
  createPasteAsInContextMenuItems: () => null,
});

export const sqlEditorProEP = createExtensionPoint<SqlEditorProFeatures>({
  id: 'editor.sql.pro',
  name: 'SQL Editor Pro',
  description:
    'Privileged SQL editor enhancements: intentions, hover, signature help, JOIN completion, paste-as-IN.',
  getDefault: () => fallbackFeatures,
});
