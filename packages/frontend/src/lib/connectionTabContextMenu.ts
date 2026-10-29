import type { NativeMenuItemDef } from './nativeContextMenu';

/** Caller-supplied labels (typically from `t()`). No hardcoded locale strings here. */
export type ConnectionTabContextMenuLabels = {
  close: string;
  closeOthers: string;
  closeAll: string;
  closeToTheRight: string;
  closeToTheLeft: string;
};

export type ConnectionTabContextMenuHandlers = {
  onClose?: () => void;
  onCloseOthers?: () => void;
  onCloseAll?: () => void;
  onCloseToTheRight?: () => void;
  onCloseToTheLeft?: () => void;
};

export type BuildConnectionTabContextMenuArgs = {
  labels: ConnectionTabContextMenuLabels;
  handlers: ConnectionTabContextMenuHandlers;
  /** When true (only one tab), closeOthers is shown disabled. */
  onlyOneTab?: boolean;
  /** Enable “close to the right” (tabs exist after this one). */
  hasTabsToRight?: boolean;
  /** Enable “close to the left” (tabs exist before this one). */
  hasTabsToLeft?: boolean;
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
 * (close / close others / close to right|left / close all).
 */
export function buildConnectionTabContextMenuItems(
  args: BuildConnectionTabContextMenuArgs,
): NativeMenuItemDef[] {
  const {
    labels,
    handlers,
    onlyOneTab = false,
    hasTabsToRight = false,
    hasTabsToLeft = false,
  } = args;

  return push(
    item('close', labels.close, handlers.onClose),
    item('close-others', labels.closeOthers, handlers.onCloseOthers, !onlyOneTab),
    item('close-to-the-right', labels.closeToTheRight, handlers.onCloseToTheRight, hasTabsToRight),
    item('close-to-the-left', labels.closeToTheLeft, handlers.onCloseToTheLeft, hasTabsToLeft),
    item('close-all', labels.closeAll, handlers.onCloseAll),
  );
}
