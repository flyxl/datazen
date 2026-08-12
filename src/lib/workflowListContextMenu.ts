import type { NativeMenuItemDef } from './nativeContextMenu';

/** Caller-supplied labels (typically from `t()`). No hardcoded locale strings here. */
export type WorkflowListContextMenuLabels = {
  open: string;
  run: string;
  delete: string;
  copyName: string;
};

export type WorkflowListContextMenuHandlers = {
  onOpen?: () => void;
  onRun?: () => void;
  onDelete?: () => void;
  onCopyName?: () => void;
};

export type BuildWorkflowListContextMenuArgs = {
  labels: WorkflowListContextMenuLabels;
  handlers: WorkflowListContextMenuHandlers;
};

/** Caller-supplied labels for a history sidebar entry. */
export type WorkflowHistoryContextMenuLabels = {
  openDetail: string;
  delete?: string;
};

export type WorkflowHistoryContextMenuHandlers = {
  onOpenDetail?: () => void;
  /** Only include when a per-entry delete API exists. */
  onDelete?: () => void;
};

export type BuildWorkflowHistoryContextMenuArgs = {
  labels: WorkflowHistoryContextMenuLabels;
  handlers: WorkflowHistoryContextMenuHandlers;
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
 * Build native context-menu items for a workflow sidebar entry
 * (open / run / delete / copy name).
 */
export function buildWorkflowListContextMenuItems(
  args: BuildWorkflowListContextMenuArgs,
): NativeMenuItemDef[] {
  const { labels, handlers } = args;
  return push(
    item('open', labels.open, handlers.onOpen),
    item('run', labels.run, handlers.onRun),
    item('delete', labels.delete, handlers.onDelete),
    item('copy-name', labels.copyName, handlers.onCopyName),
  );
}

/**
 * Build native context-menu items for a workflow history sidebar entry
 * (open detail; delete only when handler is provided).
 */
export function buildWorkflowHistoryContextMenuItems(
  args: BuildWorkflowHistoryContextMenuArgs,
): NativeMenuItemDef[] {
  const { labels, handlers } = args;
  return push(
    item('open-detail', labels.openDetail, handlers.onOpenDetail),
    item('delete', labels.delete ?? '', handlers.onDelete),
  );
}
