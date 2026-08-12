import type { NativeMenuItemDef } from './nativeContextMenu';

/** Caller-supplied labels (typically from `t()`). No hardcoded locale strings here. */
export type ConnectionTabContextMenuLabels = {
  close: string;
  closeOthers: string;
  closeAll: string;
};

export type ConnectionTabContextMenuHandlers = {
  onClose?: () => void;
  onCloseOthers?: () => void;
  onCloseAll?: () => void;
};

export type BuildConnectionTabContextMenuArgs = {
  labels: ConnectionTabContextMenuLabels;
  handlers: ConnectionTabContextMenuHandlers;
  /** When true (only one tab), closeOthers is shown disabled. */
  onlyOneTab?: boolean;
};

function item(
  id: string,
  label: string,
  action: (() => void) | undefined,
  enabled = true,
): NativeMenuItemDef | null {
  if (!action) return null;
  return { kind: 'item', id, label, action, enabled };
}

function push(...defs: Array<NativeMenuItemDef | null>): NativeMenuItemDef[] {
  return defs.filter((d): d is NativeMenuItemDef => d != null);
}

/**
 * Build native context-menu items for connection window panel tabs
 * (close / close others / close all).
 */
export function buildConnectionTabContextMenuItems(
  args: BuildConnectionTabContextMenuArgs,
): NativeMenuItemDef[] {
  const { labels, handlers, onlyOneTab = false } = args;

  return push(
    item('close', labels.close, handlers.onClose),
    item('close-others', labels.closeOthers, handlers.onCloseOthers, !onlyOneTab),
    item('close-all', labels.closeAll, handlers.onCloseAll),
  );
}
