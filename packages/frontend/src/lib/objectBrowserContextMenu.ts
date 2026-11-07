import type { NativeMenuItemDef } from './nativeContextMenu';
import { nativeEditMenuItems } from './nativeContextMenu';

export type ObjectBrowserListMenuLabels = {
  open: string;
  copyName: string;
  copyDdl: string;
  refresh: string;
};

export type ObjectBrowserListMenuHandlers = {
  onOpen?: () => void;
  onCopyName?: () => void;
  onCopyDdl?: () => void;
  onRefresh?: () => void;
};

export type ObjectBrowserEditorMenuLabels = {
  execute: string;
  format: string;
  comment: string;
};

export type ObjectBrowserEditorMenuHandlers = {
  onExecute?: () => void;
  onFormat?: () => void;
  onComment?: () => void;
};

function item(
  id: string,
  label: string,
  action: (() => void) | undefined,
  enabled = true,
): NativeMenuItemDef | null {
  if (!action) return null;
  return { kind: 'item', id, label, enabled, action };
}

function push(...defs: Array<NativeMenuItemDef | null>): NativeMenuItemDef[] {
  return defs.filter((d): d is NativeMenuItemDef => d != null);
}

/** Routine list: refresh first (tree-menu convention), then open / copy name / copy DDL. */
export function buildObjectBrowserListMenuItems(args: {
  labels: ObjectBrowserListMenuLabels;
  handlers: ObjectBrowserListMenuHandlers;
}): NativeMenuItemDef[] {
  const { labels, handlers } = args;
  return push(
    item('refresh', labels.refresh, handlers.onRefresh),
    item('open', labels.open, handlers.onOpen),
    item('copy-name', labels.copyName, handlers.onCopyName),
    item('copy-ddl', labels.copyDdl, handlers.onCopyDdl),
  );
}

/** Routine DDL editor: OS edit + execute / format / comment. */
export function buildObjectBrowserEditorMenuItems(args: {
  labels: ObjectBrowserEditorMenuLabels;
  handlers: ObjectBrowserEditorMenuHandlers;
  sqlText: string;
  hasSelection?: boolean;
}): NativeMenuItemDef[] {
  const { labels, handlers, sqlText, hasSelection = false } = args;
  const trimmed = sqlText.trim();
  return push(
    ...nativeEditMenuItems(),
    { kind: 'separator' },
    item('execute', labels.execute, handlers.onExecute, trimmed.length > 0),
    item('format', labels.format, handlers.onFormat, trimmed.length > 0),
    item('comment', labels.comment, handlers.onComment, trimmed.length > 0 || hasSelection),
  );
}
