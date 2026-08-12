import type { NativeMenuItemDef } from './nativeContextMenu';

/** Caller-supplied labels (typically from `t()`). No hardcoded locale strings here. */
export type QuerySidebarFavoriteLabels = {
  applySql: string;
  copySql: string;
  delete: string;
};

export type QuerySidebarFavoriteHandlers = {
  onApplySql?: () => void;
  onCopySql?: () => void;
  onDelete?: () => void;
};

export type BuildFavoriteSidebarContextMenuArgs = {
  labels: QuerySidebarFavoriteLabels;
  handlers: QuerySidebarFavoriteHandlers;
};

/** Caller-supplied labels (typically from `t()`). No hardcoded locale strings here. */
export type QuerySidebarHistoryLabels = {
  applySql: string;
  copySql: string;
};

export type QuerySidebarHistoryHandlers = {
  onApplySql?: () => void;
  onCopySql?: () => void;
};

export type BuildHistorySidebarContextMenuArgs = {
  labels: QuerySidebarHistoryLabels;
  handlers: QuerySidebarHistoryHandlers;
};

/** Header menu for history sidebar (e.g. clear all). */
export type QuerySidebarHistoryHeaderLabels = {
  clearHistory: string;
};

export type QuerySidebarHistoryHeaderHandlers = {
  onClearHistory?: () => void;
};

export type BuildHistorySidebarHeaderContextMenuArgs = {
  labels: QuerySidebarHistoryHeaderLabels;
  handlers: QuerySidebarHistoryHeaderHandlers;
};

function item(
  id: string,
  label: string,
  action: (() => void) | undefined,
): NativeMenuItemDef | null {
  if (!action) return null;
  return { kind: 'item', id, label, action };
}

function push(...defs: Array<NativeMenuItemDef | null>): NativeMenuItemDef[] {
  return defs.filter((d): d is NativeMenuItemDef => d != null);
}

/**
 * Build native context-menu items for a favorite SQL sidebar entry
 * (apply / copy / delete). Rename is omitted — no rename API.
 */
export function buildFavoriteSidebarContextMenuItems(
  args: BuildFavoriteSidebarContextMenuArgs,
): NativeMenuItemDef[] {
  const { labels, handlers } = args;
  return push(
    item('apply-sql', labels.applySql, handlers.onApplySql),
    item('copy-sql', labels.copySql, handlers.onCopySql),
    item('delete', labels.delete, handlers.onDelete),
  );
}

/**
 * Build native context-menu items for a history sidebar entry
 * (apply / copy). Entry-level delete is not supported.
 */
export function buildHistorySidebarContextMenuItems(
  args: BuildHistorySidebarContextMenuArgs,
): NativeMenuItemDef[] {
  const { labels, handlers } = args;
  return push(
    item('apply-sql', labels.applySql, handlers.onApplySql),
    item('copy-sql', labels.copySql, handlers.onCopySql),
  );
}

/**
 * Build native context-menu items for the history sidebar header (clear all).
 */
export function buildHistorySidebarHeaderContextMenuItems(
  args: BuildHistorySidebarHeaderContextMenuArgs,
): NativeMenuItemDef[] {
  const { labels, handlers } = args;
  return push(item('clear-history', labels.clearHistory, handlers.onClearHistory));
}
