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
  queryHistory: 'Query History',
  createDatabase: 'Create Database',
  createSchema: 'Create Schema',
  createUser: 'Create User',
  refresh: 'Refresh',
  pinConnection: 'Pin Connection',
  unpinConnection: 'Unpin Connection',
  objectFilter: 'Object Filter',
  processList: 'Process List',
  serverStatus: 'Server Status',
  backup: 'Backup',
  restore: 'Restore',
  connection: 'Connection',
  server: 'Server',
  organize: 'Manage Connection',
  createNew: 'Create New',
  database: 'Database',
  user: 'User',
};

/** Recursively find a menu item by id, searching through submenus. */
function findMenuItem(
  items: Array<{ kind: string; id?: string; items?: unknown[] }>,
  id: string,
): { kind: string; id?: string } | undefined {
  for (const item of items) {
    if (item.id === id) return item;
    if (item.items) {
      const found = findMenuItem(item.items as typeof items, id);
      if (found) return found;
    }
  }
  return undefined;
}

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

  it('includes new connection on grouped headers', () => {
    const grouped = buildMainGroupContextMenuItems({
      labels,
      isUngrouped: false,
      onNewGroup: () => undefined,
      onNewConnection: () => undefined,
      onRenameGroup: () => undefined,
      onDeleteGroup: () => undefined,
    });
    expect(grouped.map((i) => (i.kind === 'item' ? i.id : i.kind))).toEqual([
      'new-group',
      'new-connection',
      'separator',
      'rename-group',
      'delete-group',
    ]);
  });

  it('omits rename/delete for the ungrouped header', () => {
    const ungrouped = buildMainGroupContextMenuItems({
      labels,
      isUngrouped: true,
      onNewGroup: () => undefined,
      onNewConnection: () => undefined,
      onRenameGroup: () => undefined,
      onDeleteGroup: () => undefined,
    });
    expect(ungrouped.map((i) => (i.kind === 'item' ? i.id : i.kind))).toEqual([
      'new-group',
      'new-connection',
    ]);
  });

  it('puts open-connection/disconnect as the first item', () => {
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
    expect(items[0]).toMatchObject({ kind: 'item', id: 'disconnect' });
    expect(items[1]).toMatchObject({ kind: 'separator' });
  });

  it('puts copy actions inside the connection submenu', () => {
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
    expect(items.find((i) => i.kind === 'item' && i.id === 'copy-name')).toBeUndefined();
    expect(items.find((i) => i.kind === 'item' && i.id === 'copy-connection-url')).toBeUndefined();
    const connectionSub = items.find((i) => i.kind === 'submenu' && i.id === 'connection-submenu');
    expect((connectionSub as { items: Array<{ id?: string }> }).items.map((i) => i.id)).toEqual([
      'edit-connection',
      'duplicate-connection',
      'copy-connection-url',
      'copy-name',
    ]);
  });

  it('invokes onCopyUrl from the connection submenu action', () => {
    const onCopyUrl = vi.fn();
    const items = buildMainConnectionContextMenuItems({
      labels,
      isConnected: false,
      grouped: false,
      moveTargets: [],
      onOpenOrDisconnect: () => undefined,
      onCopyName: () => undefined,
      onCopyUrl,
      onNewQuery: () => undefined,
      onRefresh: () => undefined,
      onEdit: () => undefined,
      onDuplicate: () => undefined,
      onMoveToGroup: () => undefined,
      onRemoveFromGroup: () => undefined,
      onDelete: () => undefined,
    });
    const connectionSub = items.find((i) => i.kind === 'submenu' && i.id === 'connection-submenu');
    const copyUrlItem = (
      connectionSub as { items: Array<{ id?: string; action?: () => void }> }
    ).items.find((i) => i.id === 'copy-connection-url');
    copyUrlItem?.action?.();
    expect(onCopyUrl).toHaveBeenCalledTimes(1);
  });

  it('puts move targets in the organize submenu', () => {
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
      onPin: () => undefined,
      onMoveToGroup: move,
      onRemoveFromGroup: () => undefined,
      onDelete: () => undefined,
    });
    const sub = items.find((i) => i.kind === 'submenu' && i.id === 'organize-submenu');
    expect(sub?.kind).toBe('submenu');
    const prod = (sub as { items: Array<{ id?: string; action?: () => void }> })?.items?.find(
      (i) => i.id === 'move-group-Prod',
    );
    prod?.action?.();
    expect(move).toHaveBeenCalledWith('Prod');
  });

  it('puts pin and remove-from-group in the organize submenu', () => {
    const items = buildMainConnectionContextMenuItems({
      labels,
      isConnected: false,
      grouped: true,
      moveTargets: [],
      pinned: false,
      onOpenOrDisconnect: () => undefined,
      onCopyName: () => undefined,
      onCopyUrl: () => undefined,
      onNewQuery: () => undefined,
      onRefresh: () => undefined,
      onEdit: () => undefined,
      onDuplicate: () => undefined,
      onPin: () => undefined,
      onMoveToGroup: () => undefined,
      onRemoveFromGroup: () => undefined,
      onDelete: () => undefined,
    });
    const sub = items.find((i) => i.kind === 'submenu' && i.id === 'organize-submenu');
    const ids = (sub as { items: Array<{ id?: string }> }).items.map((i) => i.id);
    expect(ids).toContain('pin-connection');
    expect(ids).toContain('remove-from-group');
  });

  it('toggles disconnect/open-connection label based on isConnected', () => {
    const connected = buildMainConnectionContextMenuItems({
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
    expect(connected[0]).toMatchObject({ id: 'disconnect', label: 'Disconnect' });

    const disconnected = buildMainConnectionContextMenuItems({
      labels,
      isConnected: false,
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
    expect(disconnected[0]).toMatchObject({ id: 'open-connection', label: 'Open' });
  });

  it('puts create actions inside the create-new submenu', () => {
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
      onCreateDatabase: () => undefined,
      onCreateSchema: () => undefined,
      onCreateUser: () => undefined,
    });
    const createSub = items.find((i) => i.kind === 'submenu' && i.id === 'create-new-submenu');
    expect(createSub).toBeDefined();
    expect((createSub as { items: Array<{ id?: string; label?: string }> }).items).toEqual(
      [
        { id: 'create-database', label: 'Database' },
        { id: 'create-schema', label: 'Create Schema' },
        { id: 'create-user', label: 'User' },
      ].map((entry) => expect.objectContaining(entry)),
    );
  });

  it('hides create-new submenu when no create action is provided', () => {
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
    expect(
      items.find((i) => i.kind === 'submenu' && i.id === 'create-new-submenu'),
    ).toBeUndefined();
  });

  it('includes backup and restore in the server submenu when handlers provided', () => {
    const onBackup = vi.fn();
    const onRestore = vi.fn();
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
      onBackup,
      onRestore,
    });
    const serverSub = items.find((i) => i.kind === 'submenu' && i.id === 'server-submenu');
    expect(serverSub).toBeDefined();
    const serverIds = (serverSub as { items: Array<{ id?: string }> }).items.map((i) => i.id);
    expect(serverIds).toContain('backup');
    expect(serverIds).toContain('restore');
  });

  it('groups connection management items in the connection submenu', () => {
    const onPin = vi.fn();
    const onObjectFilter = vi.fn();
    const items = buildMainConnectionContextMenuItems({
      labels,
      isConnected: true,
      grouped: false,
      moveTargets: [],
      pinned: false,
      onOpenOrDisconnect: () => undefined,
      onCopyName: () => undefined,
      onCopyUrl: () => undefined,
      onNewQuery: () => undefined,
      onRefresh: () => undefined,
      onEdit: () => undefined,
      onDuplicate: () => undefined,
      onPin,
      onObjectFilter,
      onMoveToGroup: () => undefined,
      onRemoveFromGroup: () => undefined,
      onDelete: () => undefined,
    });
    const connectionSub = items.find((i) => i.kind === 'submenu' && i.id === 'connection-submenu');
    expect(connectionSub).toBeDefined();
    const connectionIds = (connectionSub as { items: Array<{ id?: string }> }).items.map(
      (i) => i.id,
    );
    expect(connectionIds).toEqual([
      'edit-connection',
      'duplicate-connection',
      'copy-connection-url',
      'copy-name',
      'object-filter',
    ]);
    expect(findMenuItem(items, 'pin-connection')).toBeDefined();
    expect(findMenuItem(items, 'pin-connection')?.id).toBe('pin-connection');
  });

  it('groups server items in the server submenu', () => {
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
      onProcessList: () => undefined,
      onServerStatus: () => undefined,
      onBackup: () => undefined,
      onRestore: () => undefined,
    });
    const serverSub = items.find((i) => i.kind === 'submenu' && i.id === 'server-submenu');
    expect(serverSub).toBeDefined();
    const serverIds = (serverSub as { items: Array<{ id?: string; kind?: string }> }).items.map(
      (i) => i.id ?? i.kind,
    );
    expect(serverIds).toEqual(['process-list', 'server-status', 'separator', 'backup', 'restore']);
  });

  it('top-level items are limited to primary actions, query, copy, refresh, and delete', () => {
    const items = buildMainConnectionContextMenuItems({
      labels,
      isConnected: true,
      grouped: true,
      moveTargets: [{ id: 'Prod', label: 'Prod' }],
      onOpenOrDisconnect: () => undefined,
      onCopyName: () => undefined,
      onCopyUrl: () => undefined,
      onNewQuery: () => undefined,
      onQueryHistory: () => undefined,
      onRefresh: () => undefined,
      onEdit: () => undefined,
      onDuplicate: () => undefined,
      onPin: () => undefined,
      onMoveToGroup: () => undefined,
      onRemoveFromGroup: () => undefined,
      onDelete: () => undefined,
      onCreateDatabase: () => undefined,
      onProcessList: () => undefined,
    });
    const topLevelIds = items.filter((i) => i.kind === 'item').map((i) => (i as { id: string }).id);
    expect(topLevelIds).toEqual([
      'disconnect',
      'new-query',
      'query-history',
      'refresh',
      'delete-connection',
    ]);
    const submenuIds = items
      .filter((i) => i.kind === 'submenu')
      .map((i) => (i as { id: string }).id);
    expect(submenuIds).toEqual([
      'connection-submenu',
      'server-submenu',
      'create-new-submenu',
      'organize-submenu',
    ]);
  });
});
