import type { NativeMenuItemDef } from './nativeContextMenu';

/** Caller-supplied labels (typically from `t()`). No hardcoded locale strings here. */
export type ErNodeContextMenuLabels = {
  openTable: string;
  copyName: string;
  focusTable: string;
};

export type ErNodeContextMenuHandlers = {
  onOpenTable?: () => void;
  onCopyName?: () => void;
  onFocusTable?: () => void;
};

export type BuildErNodeContextMenuArgs = {
  labels: ErNodeContextMenuLabels;
  handlers: ErNodeContextMenuHandlers;
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
 * Build native context-menu items for an ER diagram table node
 * (open table / copy name / focus — focus only when handler is provided).
 */
export function buildErNodeContextMenuItems(args: BuildErNodeContextMenuArgs): NativeMenuItemDef[] {
  const { labels, handlers } = args;
  return push(
    item('open-table', labels.openTable, handlers.onOpenTable),
    item('copy-name', labels.copyName, handlers.onCopyName),
    item('focus-table', labels.focusTable, handlers.onFocusTable),
  );
}
