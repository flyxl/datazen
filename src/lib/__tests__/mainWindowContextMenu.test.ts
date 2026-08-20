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
  copyName: 'Copy Name',
  copyConnectionUrl: 'Copy URL',
  newQuery: 'New Query',
  executeSqlFile: 'Execute SQL File',
  createDatabase: 'Create Database',
  createSchema: 'Create Schema',
  createUser: 'Create User',
  refresh: 'Refresh',
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

  it('puts refresh first on connection menu', () => {
    const items = buildMainConnectionContextMenuItems({
      labels,
      isConnected: true,
      grouped: false,
      moveTargets: [],
      onOpenOrDisconnect: () => undefined,
      onCopyName: () => undefined,
      onCopyUrl: () => undefined,
      onNewQuery: () => undefined,
      onRefresh: () => undefined,
      onEdit: () => undefined,
      onDuplicate: () => undefined,
      onMoveToGroup: () => undefined,
      onRemoveFromGroup: () => undefined,
      onDelete: () => undefined,
    });
    expect(items[0]).toMatchObject({ kind: 'item', id: 'refresh' });
    expect(items[1]).toMatchObject({ kind: 'separator' });
    expect(items[2]).toMatchObject({ kind: 'item', id: 'disconnect' });
  });

  it('puts move targets in a submenu so they can flip at the window edge', () => {
    const move = vi.fn();
    const items = buildMainConnectionContextMenuItems({
      labels,
      isConnected: false,
      grouped: true,
      moveTargets: [{ id: 'Prod', label: 'Prod' }],
      onOpenOrDisconnect: () => undefined,
      onCopyName: () => undefined,
      onCopyUrl: () => undefined,
      onNewQuery: () => undefined,
      onRefresh: () => undefined,
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

  it('includes execute-sql-file when handler provided and omits when not', () => {
    const baseArgs = {
      labels,
      isConnected: true,
      grouped: false,
      moveTargets: [],
      onOpenOrDisconnect: () => undefined,
      onCopyName: () => undefined,
      onCopyUrl: () => undefined,
      onNewQuery: () => undefined,
      onRefresh: () => undefined,
      onEdit: () => undefined,
      onDuplicate: () => undefined,
      onMoveToGroup: () => undefined,
      onRemoveFromGroup: () => undefined,
      onDelete: () => undefined,
    };

    const withSql = buildMainConnectionContextMenuItems({
      ...baseArgs,
      onExecuteSqlFile: vi.fn(),
    });
    expect(withSql.some((i) => i.kind === 'item' && i.id === 'execute-sql-file')).toBe(true);

    const withoutSql = buildMainConnectionContextMenuItems(baseArgs);
    expect(withoutSql.some((i) => i.kind === 'item' && i.id === 'execute-sql-file')).toBe(false);
  });

  it('includes create-database/schema/user when handlers provided', () => {
    const onCreateDb = vi.fn();
    const onCreateSchema = vi.fn();
    const onCreateUser = vi.fn();
    const items = buildMainConnectionContextMenuItems({
      labels,
      isConnected: true,
      grouped: false,
      moveTargets: [],
      onOpenOrDisconnect: () => undefined,
      onCopyName: () => undefined,
      onCopyUrl: () => undefined,
      onNewQuery: () => undefined,
      onRefresh: () => undefined,
      onEdit: () => undefined,
      onDuplicate: () => undefined,
      onMoveToGroup: () => undefined,
      onRemoveFromGroup: () => undefined,
      onDelete: () => undefined,
      onCreateDatabase: onCreateDb,
      onCreateSchema: onCreateSchema,
      onCreateUser: onCreateUser,
    });
    const dbItem = items.find((i) => i.kind === 'item' && i.id === 'create-database');
    expect(dbItem).toBeDefined();
    if (dbItem?.kind === 'item') dbItem.action();
    expect(onCreateDb).toHaveBeenCalledOnce();

    const schemaItem = items.find((i) => i.kind === 'item' && i.id === 'create-schema');
    expect(schemaItem).toBeDefined();
    if (schemaItem?.kind === 'item') schemaItem.action();
    expect(onCreateSchema).toHaveBeenCalledOnce();

    const userItem = items.find((i) => i.kind === 'item' && i.id === 'create-user');
    expect(userItem).toBeDefined();
    if (userItem?.kind === 'item') userItem.action();
    expect(onCreateUser).toHaveBeenCalledOnce();
  });

  it('omits admin items when none of the admin handlers are provided', () => {
    const items = buildMainConnectionContextMenuItems({
      labels,
      isConnected: true,
      grouped: false,
      moveTargets: [],
      onOpenOrDisconnect: () => undefined,
      onCopyName: () => undefined,
      onCopyUrl: () => undefined,
      onNewQuery: () => undefined,
      onRefresh: () => undefined,
      onEdit: () => undefined,
      onDuplicate: () => undefined,
      onMoveToGroup: () => undefined,
      onRemoveFromGroup: () => undefined,
      onDelete: () => undefined,
    });
    expect(items.some((i) => i.kind === 'item' && i.id === 'create-database')).toBe(false);
    expect(items.some((i) => i.kind === 'item' && i.id === 'create-schema')).toBe(false);
    expect(items.some((i) => i.kind === 'item' && i.id === 'create-user')).toBe(false);
  });
});
