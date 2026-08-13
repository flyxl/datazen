import { describe, expect, it, vi } from 'vitest';
import {
  buildMainBlankContextMenuItems,
  buildMainConnectionContextMenuItems,
  buildMainGroupContextMenuItems,
} from '../mainWindowContextMenu';

const labels = {
  newGroup: 'New group',
  newConnection: 'New connection',
  renameGroup: 'Rename',
  deleteGroup: 'Delete group',
  openConnection: 'Open',
  disconnect: 'Disconnect',
  editConnection: 'Edit',
  duplicateConnection: 'Duplicate',
  moveToGroup: 'Move to group',
  removeFromGroup: 'Remove from group',
  deleteConnection: 'Delete',
};

describe('mainWindowContextMenu', () => {
  it('builds blank-area items', () => {
    const items = buildMainBlankContextMenuItems({
      labels,
      onNewGroup: () => undefined,
      onNewConnection: () => undefined,
    });
    expect(items.map((i) => (i.kind === 'item' ? i.id : i.kind))).toEqual([
      'new-group',
      'new-connection',
    ]);
  });

  it('omits rename/delete for the ungrouped header', () => {
    const ungrouped = buildMainGroupContextMenuItems({
      labels,
      isUngrouped: true,
      onNewGroup: () => undefined,
      onRenameGroup: () => undefined,
      onDeleteGroup: () => undefined,
    });
    expect(ungrouped).toHaveLength(1);
    const grouped = buildMainGroupContextMenuItems({
      labels,
      isUngrouped: false,
      onNewGroup: () => undefined,
      onRenameGroup: () => undefined,
      onDeleteGroup: () => undefined,
    });
    expect(grouped.some((i) => i.kind === 'item' && i.id === 'rename-group')).toBe(true);
  });

  it('puts move targets in a submenu so they can flip at the window edge', () => {
    const move = vi.fn();
    const items = buildMainConnectionContextMenuItems({
      labels,
      isConnected: false,
      grouped: true,
      moveTargets: [{ id: 'Prod', label: 'Prod' }],
      onOpenOrDisconnect: () => undefined,
      onEdit: () => undefined,
      onDuplicate: () => undefined,
      onMoveToGroup: move,
      onRemoveFromGroup: () => undefined,
      onDelete: () => undefined,
    });
    const sub = items.find((i) => i.kind === 'submenu');
    expect(sub?.kind).toBe('submenu');
    if (sub?.kind !== 'submenu') return;
    expect(sub.id).toBe('move-to-group');
    expect(sub.items.some((i) => i.kind === 'item' && i.id === 'move-group-Prod')).toBe(true);
    expect(sub.items.some((i) => i.kind === 'item' && i.id === 'remove-from-group')).toBe(true);
    const prod = sub.items.find((i) => i.kind === 'item' && i.id === 'move-group-Prod');
    if (prod?.kind === 'item') prod.action();
    expect(move).toHaveBeenCalledWith('Prod');
  });
});
