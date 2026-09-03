import type { NativeMenuItemDef } from './nativeContextMenu';
import { nativeEditMenuItems } from './nativeContextMenu';

export type SqlEditorContextMenuLabels = {
  run: string;
  runSelection: string;
  format: string;
  comment: string;
  addFavorite?: string;
};

export type SqlEditorContextMenuHandlers = {
  onRun?: () => void;
  onRunSelection?: () => void;
  onFormat?: () => void;
  onComment?: () => void;
  onAddFavorite?: (sql: string) => void;
};

export type BuildSqlEditorContextMenuArgs = {
  labels: SqlEditorContextMenuLabels;
  handlers: SqlEditorContextMenuHandlers;
  /** Full editor SQL (or trimmed selection fallback from the editor). */
  sqlText: string;
  /** True when the editor has a non-empty selection. */
  hasSelection?: boolean;
};

/**
 * Toggle `-- ` line comments on each non-empty line.
 * If every non-empty line is already commented, strips the comment prefix.
 */
export function toggleSqlLineComments(text: string): string {
  const lines = text.split('\n');
  const nonEmpty = lines.filter((l) => l.trim().length > 0);
  const allCommented = nonEmpty.length > 0 && nonEmpty.every((l) => /^\s*--/.test(l));
  if (allCommented) {
    return lines.map((l) => l.replace(/^(\s*)-- ?/, '$1')).join('\n');
  }
  return lines.map((l) => (l.trim().length === 0 ? l : `-- ${l}`)).join('\n');
}

/**
 * Build native context-menu items for the SQL editor.
 * OS edit actions first; then Run / Run Selection / Format / Comment; then Add Favorite.
 */
export function buildSqlEditorContextMenuItems(
  args: BuildSqlEditorContextMenuArgs,
): NativeMenuItemDef[] {
  const { labels, handlers, sqlText, hasSelection = false } = args;
  const isMac = typeof navigator !== 'undefined' && /Mac/i.test(navigator.platform || navigator.userAgent);
  const trimmed = sqlText.trim();
  const items: NativeMenuItemDef[] = [...nativeEditMenuItems(), { kind: 'separator' }];

  if (handlers.onRun) {
    items.push({
      kind: 'item',
      id: 'run',
      label: labels.run,
      shortcut: isMac ? '⌘ Enter' : 'Ctrl+Enter',
      enabled: trimmed.length > 0,
      action: handlers.onRun,
    });
  }
  if (handlers.onRunSelection) {
    items.push({
      kind: 'item',
      id: 'run-selection',
      label: labels.runSelection,
      shortcut: isMac ? '⌘ Enter' : 'Ctrl+Enter',
      enabled: hasSelection,
      action: handlers.onRunSelection,
    });
  }
  if (handlers.onFormat) {
    items.push({
      kind: 'item',
      id: 'format',
      label: labels.format,
      enabled: trimmed.length > 0,
      action: handlers.onFormat,
    });
  }
  if (handlers.onComment) {
    items.push({
      kind: 'item',
      id: 'comment',
      label: labels.comment,
      shortcut: isMac ? '⌘ /' : 'Ctrl+/',
      enabled: trimmed.length > 0 || hasSelection,
      action: handlers.onComment,
    });
  }

  if (handlers.onAddFavorite && labels.addFavorite) {
    items.push({ kind: 'separator' });
    items.push({
      kind: 'item',
      id: 'add-favorite',
      label: labels.addFavorite,
      shortcut: isMac ? '⌘ S' : 'Ctrl+S',
      enabled: trimmed.length > 0,
      action: () => {
        if (trimmed) handlers.onAddFavorite?.(trimmed);
      },
    });
  }

  return items;
}
