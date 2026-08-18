import type { NativeMenuItemDef } from './nativeContextMenu';

export type MainWindowContextMenuLabels = {
  newGroup: string;
  newConnection: string;
  renameGroup: string;
  deleteGroup: string;
  openConnection: string;
  disconnect: string;
  editConnection: string;
  duplicateConnection: string;
  moveToGroup: string;
  removeFromGroup: string;
  deleteConnection: string;
  copyName: string;
  newQuery: string;
  refresh: string;
};

export function buildMainBlankContextMenuItems(args: {
  labels: MainWindowContextMenuLabels;
  onNewGroup: () => void;
  onNewConnection: () => void;
}): NativeMenuItemDef[] {
  return [
    { kind: 'item', id: 'new-group', label: args.labels.newGroup, action: args.onNewGroup },
    {
      kind: 'item',
      id: 'new-connection',
      label: args.labels.newConnection,
      action: args.onNewConnection,
    },
  ];
}

export function buildMainGroupContextMenuItems(args: {
  labels: MainWindowContextMenuLabels;
  isUngrouped: boolean;
  onNewGroup: () => void;
  onRenameGroup: () => void;
  onDeleteGroup: () => void;
}): NativeMenuItemDef[] {
  const items: NativeMenuItemDef[] = [
    { kind: 'item', id: 'new-group', label: args.labels.newGroup, action: args.onNewGroup },
  ];
  if (!args.isUngrouped) {
    items.push(
      { kind: 'separator' },
      {
        kind: 'item',
        id: 'rename-group',
        label: args.labels.renameGroup,
        action: args.onRenameGroup,
      },
      {
        kind: 'item',
        id: 'delete-group',
        label: args.labels.deleteGroup,
        action: args.onDeleteGroup,
      },
    );
  }
  return items;
}

export function buildMainConnectionContextMenuItems(args: {
  labels: MainWindowContextMenuLabels;
  isConnected: boolean;
  grouped: boolean;
  moveTargets: Array<{ id: string; label: string }>;
  onOpenOrDisconnect: () => void;
  onCopyName: () => void;
  onNewQuery: () => void;
  onRefresh: () => void;
  onEdit: () => void;
  onDuplicate: () => void;
  onMoveToGroup: (groupId: string) => void;
  onRemoveFromGroup: () => void;
  onDelete: () => void | Promise<void>;
}): NativeMenuItemDef[] {
  const items: NativeMenuItemDef[] = [
    {
      kind: 'item',
      id: args.isConnected ? 'disconnect' : 'open-connection',
      label: args.isConnected ? args.labels.disconnect : args.labels.openConnection,
      action: args.onOpenOrDisconnect,
    },
    { kind: 'item', id: 'copy-name', label: args.labels.copyName, action: args.onCopyName },
    { kind: 'item', id: 'new-query', label: args.labels.newQuery, action: args.onNewQuery },
    { kind: 'separator' },
    { kind: 'item', id: 'refresh', label: args.labels.refresh, action: args.onRefresh },
    { kind: 'item', id: 'edit-connection', label: args.labels.editConnection, action: args.onEdit },
    {
      kind: 'item',
      id: 'duplicate-connection',
      label: args.labels.duplicateConnection,
      action: args.onDuplicate,
    },
    { kind: 'separator' },
  ];

  if (args.moveTargets.length > 0 || args.grouped) {
    const sub: NativeMenuItemDef[] = args.moveTargets.map((g) => ({
      kind: 'item' as const,
      id: `move-group-${g.id}`,
      label: g.label,
      action: () => args.onMoveToGroup(g.id),
    }));
    if (args.grouped) {
      sub.push({
        kind: 'item',
        id: 'remove-from-group',
        label: args.labels.removeFromGroup,
        action: args.onRemoveFromGroup,
      });
    }
    items.push({
      kind: 'submenu',
      id: 'move-to-group',
      label: args.labels.moveToGroup,
      items: sub,
    });
    items.push({ kind: 'separator' });
  }

  items.push({
    kind: 'item',
    id: 'delete-connection',
    label: args.labels.deleteConnection,
    action: args.onDelete,
  });
  return items;
}
