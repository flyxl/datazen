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
  copyConnectionUrl: string;
  newQuery: string;
  queryHistory: string;
  createDatabase: string;
  createSchema: string;
  createUser: string;
  refresh: string;
  pinConnection: string;
  unpinConnection: string;
  objectFilter: string;
  processList: string;
  serverStatus: string;
  backup: string;
  restore: string;
  /** Submenu header labels */
  connection: string;
  server: string;
  organize: string;
  createNew: string;
  /** Short label for create-database inside the create-new submenu */
  database: string;
  /** Short label for create-user inside the create-new submenu */
  user: string;
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
  onNewConnection: () => void;
  onRenameGroup: () => void;
  onDeleteGroup: () => void;
}): NativeMenuItemDef[] {
  const items: NativeMenuItemDef[] = [
    { kind: 'item', id: 'new-group', label: args.labels.newGroup, action: args.onNewGroup },
    {
      kind: 'item',
      id: 'new-connection',
      label: args.labels.newConnection,
      action: args.onNewConnection,
    },
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
  pinned?: boolean;
  moveTargets: Array<{ id: string; label: string }>;
  onOpenOrDisconnect: () => void;
  onCopyName: () => void;
  onCopyUrl: () => void;
  onNewQuery: () => void;
  onQueryHistory?: () => void;
  onCreateDatabase?: () => void;
  onCreateSchema?: () => void;
  onCreateUser?: () => void;
  onPin?: () => void;
  onObjectFilter?: () => void;
  onProcessList?: () => void;
  onServerStatus?: () => void;
  onBackup?: () => void;
  onRestore?: () => void;
  onRefresh: () => void;
  onEdit: () => void;
  onDuplicate: () => void;
  onMoveToGroup: (groupId: string) => void;
  onRemoveFromGroup: () => void;
  onDelete: () => void | Promise<void>;
}): NativeMenuItemDef[] {
  const items: NativeMenuItemDef[] = [];

  // ── Primary actions ────────────────────────────────────────
  items.push({
    kind: 'item',
    id: args.isConnected ? 'disconnect' : 'open-connection',
    label: args.isConnected ? args.labels.disconnect : args.labels.openConnection,
    action: args.onOpenOrDisconnect,
  });

  items.push({ kind: 'separator' });

  // ── Query actions ──────────────────────────────────────────
  items.push({
    kind: 'item',
    id: 'new-query',
    label: args.labels.newQuery,
    action: args.onNewQuery,
  });

  if (args.onQueryHistory) {
    items.push({
      kind: 'item',
      id: 'query-history',
      label: args.labels.queryHistory,
      action: args.onQueryHistory,
    });
  }

  items.push({ kind: 'separator' });

  // ── Connection submenu (edit / duplicate / copy / filter) ──
  const connectionItems: NativeMenuItemDef[] = [
    { kind: 'item', id: 'edit-connection', label: args.labels.editConnection, action: args.onEdit },
    {
      kind: 'item',
      id: 'duplicate-connection',
      label: args.labels.duplicateConnection,
      action: args.onDuplicate,
    },
    {
      kind: 'item',
      id: 'copy-connection-url',
      label: args.labels.copyConnectionUrl,
      action: args.onCopyUrl,
    },
    { kind: 'item', id: 'copy-name', label: args.labels.copyName, action: args.onCopyName },
  ];
  if (args.onObjectFilter) {
    connectionItems.push({
      kind: 'item',
      id: 'object-filter',
      label: args.labels.objectFilter,
      action: args.onObjectFilter,
    });
  }
  items.push({
    kind: 'submenu',
    id: 'connection-submenu',
    label: args.labels.connection,
    items: connectionItems,
  });

  // ── Server submenu (monitoring + backup/restore) ───────────
  const monitorItems: NativeMenuItemDef[] = [];
  const opsItems: NativeMenuItemDef[] = [];
  if (args.onProcessList) {
    monitorItems.push({
      kind: 'item',
      id: 'process-list',
      label: args.labels.processList,
      action: args.onProcessList,
    });
  }
  if (args.onServerStatus) {
    monitorItems.push({
      kind: 'item',
      id: 'server-status',
      label: args.labels.serverStatus,
      action: args.onServerStatus,
    });
  }
  if (args.onBackup) {
    opsItems.push({
      kind: 'item',
      id: 'backup',
      label: args.labels.backup,
      action: args.onBackup,
    });
  }
  if (args.onRestore) {
    opsItems.push({
      kind: 'item',
      id: 'restore',
      label: args.labels.restore,
      action: args.onRestore,
    });
  }
  const serverItems: NativeMenuItemDef[] = [...monitorItems];
  if (monitorItems.length > 0 && opsItems.length > 0) {
    serverItems.push({ kind: 'separator' });
  }
  serverItems.push(...opsItems);
  if (serverItems.length > 0) {
    items.push({
      kind: 'submenu',
      id: 'server-submenu',
      label: args.labels.server,
      items: serverItems,
    });
  }

  // ── Create submenu (database / schema / user) ──────────────
  if (args.onCreateDatabase || args.onCreateSchema || args.onCreateUser) {
    const createItems: NativeMenuItemDef[] = [];
    if (args.onCreateDatabase) {
      createItems.push({
        kind: 'item',
        id: 'create-database',
        label: args.labels.database,
        action: args.onCreateDatabase,
      });
    }
    if (args.onCreateSchema) {
      createItems.push({
        kind: 'item',
        id: 'create-schema',
        label: args.labels.createSchema,
        action: args.onCreateSchema,
      });
    }
    if (args.onCreateUser) {
      createItems.push({
        kind: 'item',
        id: 'create-user',
        label: args.labels.user,
        action: args.onCreateUser,
      });
    }
    items.push({
      kind: 'submenu',
      id: 'create-new-submenu',
      label: args.labels.createNew,
      items: createItems,
    });
  }

  // ── Bottom section: refresh / organize / delete ────────────
  items.push({ kind: 'separator' });
  items.push({ kind: 'item', id: 'refresh', label: args.labels.refresh, action: args.onRefresh });

  const organizeItems: NativeMenuItemDef[] = [];
  if (args.onPin) {
    organizeItems.push({
      kind: 'item',
      id: args.pinned ? 'unpin-connection' : 'pin-connection',
      label: args.pinned ? args.labels.unpinConnection : args.labels.pinConnection,
      action: args.onPin,
    });
  }
  const moveItems: NativeMenuItemDef[] = args.moveTargets.map((g) => ({
    kind: 'item' as const,
    id: `move-group-${g.id}`,
    label: g.label,
    action: () => args.onMoveToGroup(g.id),
  }));
  if (args.grouped) {
    moveItems.push({
      kind: 'item',
      id: 'remove-from-group',
      label: args.labels.removeFromGroup,
      action: args.onRemoveFromGroup,
    });
  }
  if (moveItems.length > 0) {
    if (organizeItems.length > 0) organizeItems.push({ kind: 'separator' });
    organizeItems.push(...moveItems);
  }
  if (organizeItems.length > 0) {
    items.push({
      kind: 'submenu',
      id: 'organize-submenu',
      label: args.labels.organize,
      items: organizeItems,
    });
  }

  items.push({ kind: 'separator' });
  items.push({
    kind: 'item',
    id: 'delete-connection',
    label: args.labels.deleteConnection,
    action: args.onDelete,
  });
  return items;
}
