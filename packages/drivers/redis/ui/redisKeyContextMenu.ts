import type { NativeMenuItemDef } from '../../../../src/lib/nativeContextMenu';

/** Caller-supplied labels (typically from `t()`). No hardcoded locale strings here. */
export type RedisKeyContextMenuLabels = {
  copyKey: string;
  setTtl: string;
  rename: string;
  delete: string;
};

export type RedisKeyContextMenuHandlers = {
  onCopyKey?: () => void;
  onSetTtl?: () => void;
  onRename?: () => void;
  onDelete?: () => void;
};

export type BuildRedisKeyContextMenuArgs = {
  labels: RedisKeyContextMenuLabels;
  handlers: RedisKeyContextMenuHandlers;
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
 * Build web context-menu items for a Redis key list row
 * (copy name / set TTL / rename / delete).
 */
export function buildRedisKeyContextMenuItems(
  args: BuildRedisKeyContextMenuArgs,
): NativeMenuItemDef[] {
  const { labels, handlers } = args;
  return push(
    item('copy-key', labels.copyKey, handlers.onCopyKey),
    item('set-ttl', labels.setTtl, handlers.onSetTtl),
    item('rename', labels.rename, handlers.onRename),
    item('delete', labels.delete, handlers.onDelete),
  );
}
