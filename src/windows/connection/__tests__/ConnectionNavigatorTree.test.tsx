import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, waitFor, fireEvent } from '@testing-library/react';
import { createRef } from 'react';
import {
  ConnectionNavigatorTree,
  type ConnectionNavigatorTreeHandle,
} from '../ConnectionNavigatorTree';
import { useSchemaStore } from '../../../stores/schemaStore';
import type { ConnectionConfig } from '../../../types';

const confirmMock = vi.hoisted(() => vi.fn().mockResolvedValue(true));

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../hooks/useConfirmDialog', () => ({
  useConfirmDialog: () => [confirmMock, null],
}));

vi.mock('../../../stores/settingsStore', () => ({
  useSettingsStore: (sel: (s: { settings: { safeMode: boolean } }) => unknown) =>
    sel({ settings: { safeMode: false } }),
}));

vi.mock('../../../stores/contextMenuStore', () => ({
  showWebContextMenu: vi.fn(),
}));

vi.mock('../../../lib/windowManager', () => ({
  openDataSyncWindow: vi.fn(),
  openSchemaDiffWindow: vi.fn(),
}));

vi.mock('../../../commands/connection', () => ({
  connectionCommands: { reorderConnections: vi.fn().mockResolvedValue(undefined) },
}));

vi.mock('../../../plugins/generated', () => {
  const sqlMulti = {
    label: 'SQL',
    shortLabel: 'SQL',
    iconBg: 'bg-blue-600',
    iconColor: 'text-blue-400',
    defaultPort: 5432,
    defaultHost: '127.0.0.1',
    defaultUser: '',
    quoteChar: '"',
    connectionMode: 'server',
    supportsSSH: true,
    supportsSSL: true,
    supportsBackup: true,
    supportsTables: true,
    isKeyValue: false,
    supportsSQL: true,
    category: 'sql',
    connectionView: 'sql',
    sqlDialect: 'postgresql',
    databaseFieldType: 'name',
    connectionForm: 'standard',
    supportsExplain: true,
    hasMultiDatabase: true,
    supportsCreateDatabase: true,
    supportsCreateSchema: true,
    defaultSchema: 'public',
  };
  const DRIVER_DB_ENTRIES = {
    postgresql: {
      ...sqlMulti,
      label: 'PostgreSQL',
      quoteChar: '"',
      sqlDialect: 'postgresql',
      defaultSchema: 'public',
    },
    mysql: {
      ...sqlMulti,
      label: 'MySQL',
      quoteChar: '`',
      sqlDialect: 'mysql',
      defaultPort: 3306,
    },
  };
  return {
    DRIVER_DB_ENTRIES,
    PLUGIN_DB_ENTRIES: DRIVER_DB_ENTRIES,
    DRIVER_ICON_ENTRIES: {},
    DRIVER_ICON_PARENTS: {},
    PLUGIN_SQL_DIALECTS: {},
    getPluginSchemaTree: () => undefined,
    getPluginConnectionForm: () => undefined,
    getPluginConnectionAdvanced: () => undefined,
    getPluginValidator: () => undefined,
    getPluginClipboardParsers: () => [],
  };
});

vi.mock('@tanstack/react-virtual', () => ({
  useVirtualizer: ({
    count,
    estimateSize,
  }: {
    count: number;
    estimateSize?: (i: number) => number;
  }) => {
    const sizeOf = (i: number) => estimateSize?.(i) ?? 28;
    let offset = 0;
    const items = Array.from({ length: count }, (_, index) => {
      const size = sizeOf(index);
      const start = offset;
      offset += size;
      return { index, key: index, start, size, end: start + size };
    });
    return {
      getTotalSize: () => offset || count * 28,
      getVirtualItems: () => items,
    };
  },
}));

const mockGetDatabases = vi.fn();
const mockGetTables = vi.fn();
const mockUseDatabase = vi.fn();
const mockDriverExecute = vi.fn();

vi.mock('../../../commands/database', () => ({
  databaseCommands: {
    getDatabases: (...args: unknown[]) => mockGetDatabases(...args),
    getTables: (...args: unknown[]) => mockGetTables(...args),
    useDatabase: (...args: unknown[]) => mockUseDatabase(...args),
    getDatabaseObjects: vi.fn().mockResolvedValue([]),
  },
}));

vi.mock('../../../commands/driver', () => ({
  driverCommands: {
    execute: (...args: unknown[]) => mockDriverExecute(...args),
    getDriverCommands: vi.fn().mockResolvedValue([]),
  },
}));

const MYSQL_CONN: ConnectionConfig = {
  id: 'cfg-mysql',
  name: 'Local MySQL',
  databaseType: 'mysql',
  host: '127.0.0.1',
  port: 3306,
  sslMode: 'disable',
  group: '',
};

const connectionsState = {
  connections: [MYSQL_CONN],
  groups: [''],
  duplicateConnection: vi.fn(),
  addGroup: vi.fn(),
  deleteGroup: vi.fn(),
  renameGroup: vi.fn(),
  moveConnectionToGroup: vi.fn(),
};

const activeConnectionsState = {
  connections: {
    'cfg-mysql': { status: 'connected' as const, dbSessionId: 'conn-1', connectionId: 'cfg-mysql' },
  },
};

vi.mock('../../../stores/connectionStore', () => ({
  useConnectionStore: (sel: (s: typeof connectionsState) => unknown) => sel(connectionsState),
  groupConnections: (connections: ConnectionConfig[], groups: string[], _query: string) => [
    { group: '', connections },
  ],
  groupConnectionsWithPinnedSection: (
    connections: ConnectionConfig[],
    _groups: string[],
    _query: string,
  ) => [{ group: '', connections }],
  PINNED_GROUP_KEY: '__pinned__',
}));

vi.mock('../../../stores/activeConnectionStore', () => ({
  useActiveConnectionStore: Object.assign(
    (sel: (s: typeof activeConnectionsState & { connect: () => void }) => unknown) =>
      sel({ ...activeConnectionsState, connect: vi.fn() }),
    {
      getState: () => ({ ...activeConnectionsState, connect: vi.fn() }),
    },
  ),
}));

async function ensureDbTableVisible(
  findByText: (text: string) => Promise<HTMLElement>,
  queryAllByText: (text: string) => HTMLElement[],
  dbName: string,
  tableName: string,
) {
  await waitFor(() => findByText(dbName));

  if (queryAllByText(tableName).length > 0) return;

  fireEvent.click((await findByText(dbName)).closest('button')!);
  await waitFor(() => {
    expect(mockGetTables).toHaveBeenCalledWith('conn-1', dbName);
    expect(queryAllByText(tableName).length).toBeGreaterThan(0);
  });
}

async function triggerContextMenuAction(
  element: HTMLElement,
  actionId: string,
): Promise<ReturnType<typeof vi.fn>> {
  fireEvent.contextMenu(element);
  const { showWebContextMenu } = await import('../../../stores/contextMenuStore');
  await waitFor(() => {
    const items = vi.mocked(showWebContextMenu).mock.calls.at(-1)?.[0] ?? [];
    expect(items.some((item) => item.id === actionId)).toBe(true);
  });
  const menuItems = vi.mocked(showWebContextMenu).mock.calls.at(-1)?.[0] ?? [];
  const target = menuItems.find((item) => item.id === actionId);
  expect(target).toBeDefined();
  target?.action?.();
  return vi.mocked(showWebContextMenu);
}

async function triggerDropDatabase(
  findByText: (text: string) => Promise<HTMLElement>,
  dbName: string,
) {
  await waitFor(() => findByText(dbName));
  await triggerContextMenuAction((await findByText(dbName)).closest('button')!, 'drop-database');
}

async function triggerContextMenuRefresh(element: HTMLElement): Promise<void> {
  const { showWebContextMenu } = await import('../../../stores/contextMenuStore');
  fireEvent.contextMenu(element);
  // The connection-level handler awaits driver command discovery before
  // showing the menu — poll for the refresh item instead of reading syncly.
  await waitFor(() => {
    const items = vi.mocked(showWebContextMenu).mock.calls.at(-1)?.[0] ?? [];
    expect(items.some((item) => item.id === 'refresh')).toBe(true);
  });
  const menuItems = vi.mocked(showWebContextMenu).mock.calls.at(-1)?.[0] ?? [];
  const refreshItem = menuItems.find((item) => item.id === 'refresh');
  expect(refreshItem).toBeDefined();
  refreshItem?.action?.();
}

async function triggerConnectionRefresh(findByText: (text: string) => Promise<HTMLElement>) {
  const connLabel = await findByText('Local MySQL');
  const connRow = connLabel.closest('[data-conn-item]')!;
  await triggerContextMenuRefresh(connRow);
}

async function triggerDatabaseRefresh(
  findByText: (text: string) => Promise<HTMLElement>,
  dbName: string,
) {
  await waitFor(() => findByText(dbName));
  await triggerContextMenuRefresh((await findByText(dbName)).closest('button')!);
}

async function triggerSchemaRefresh(
  findByText: (text: string) => Promise<HTMLElement>,
  schemaName: string,
) {
  await waitFor(() => findByText(schemaName));
  await triggerContextMenuRefresh((await findByText(schemaName)).closest('button')!);
}

const baseProps = {
  activeConnectionId: 'cfg-mysql',
  onSelectConnection: vi.fn(),
  onSelectTable: vi.fn(),
  onNewConnection: vi.fn(),
  onEditConnection: vi.fn(),
  onDeleteConnection: vi.fn(),
  onDisconnect: vi.fn(),
};

afterEach(() => {
  cleanup();
  useSchemaStore.getState().reset();
  vi.clearAllMocks();
  confirmMock.mockResolvedValue(true);
});

beforeEach(() => {
  mockGetDatabases.mockResolvedValue(['db_a', 'db_b', 'postgres']);
  mockGetTables.mockImplementation((_connId: string, dbName: string) => {
    if (dbName === 'db_a') {
      return Promise.resolve([{ name: 'users', tableType: 'table', schema: null, rowCount: null }]);
    }
    if (dbName === 'db_b') {
      return Promise.resolve([
        { name: 'orders', tableType: 'table', schema: null, rowCount: null },
      ]);
    }
    return Promise.resolve([]);
  });
  mockUseDatabase.mockResolvedValue(undefined);
  mockDriverExecute.mockResolvedValue({});
  useSchemaStore.getState().reset();
  useSchemaStore.getState().setActiveConnection('conn-1');
});

describe('ConnectionNavigatorTree active connection highlight', () => {
  it('highlights only the row matching the activeConnectionId prop', async () => {
    connectionsState.connections = [
      MYSQL_CONN,
      { ...MYSQL_CONN, id: 'cfg-pg', name: 'Local PG', databaseType: 'postgresql', port: 5432 },
    ];
    activeConnectionsState.connections = {
      'cfg-mysql': {
        status: 'connected' as const,
        dbSessionId: 'conn-1',
        connectionId: 'cfg-mysql',
      },
      'cfg-pg': { status: 'connected' as const, dbSessionId: 'conn-2', connectionId: 'cfg-pg' },
    };

    const view = render(<ConnectionNavigatorTree {...baseProps} activeConnectionId="cfg-pg" />);

    const pgRow = await waitFor(() => {
      const el = view.container.querySelector<HTMLElement>('[data-conn-name="Local PG"]');
      expect(el).not.toBeNull();
      return el!;
    });
    // Selected style + left accent bar come solely from the activeConnectionId prop.
    expect(pgRow.className).toContain('bg-accent/10');
    expect(pgRow.querySelector('span.absolute')).not.toBeNull();
    const mysqlRow = view.container.querySelector<HTMLElement>('[data-conn-name="Local MySQL"]')!;
    expect(mysqlRow.className).not.toContain('bg-accent/10');

    // Flipping the prop moves the highlight — proves the assertion can tell
    // a correctly-wired prop from a stale/ignored one.
    view.rerender(<ConnectionNavigatorTree {...baseProps} activeConnectionId="cfg-mysql" />);
    await waitFor(() => {
      const row = view.container.querySelector<HTMLElement>('[data-conn-name="Local MySQL"]');
      expect(row?.className).toContain('bg-accent/10');
    });
    const pgAfter = view.container.querySelector<HTMLElement>('[data-conn-name="Local PG"]')!;
    expect(pgAfter.className).not.toContain('bg-accent/10');

    connectionsState.connections = [MYSQL_CONN];
    activeConnectionsState.connections = {
      'cfg-mysql': {
        status: 'connected' as const,
        dbSessionId: 'conn-1',
        connectionId: 'cfg-mysql',
      },
    };
  });
});

describe('ConnectionNavigatorTree multi-db table selection', () => {
  it('activates the table database before opening when another db was active', async () => {
    const onSelectTable = vi.fn();
    const { findByText, queryAllByText } = render(
      <ConnectionNavigatorTree {...baseProps} onSelectTable={onSelectTable} />,
    );

    await ensureDbTableVisible(findByText, queryAllByText, 'db_a', 'users');
    await ensureDbTableVisible(findByText, queryAllByText, 'db_b', 'orders');

    await waitFor(() => {
      expect(useSchemaStore.getState().currentDatabase).toBe('db_b');
    });

    mockUseDatabase.mockClear();

    fireEvent.click((await findByText('users')).closest('button')!);

    await waitFor(() => {
      expect(mockUseDatabase).toHaveBeenCalledWith('conn-1', 'db_a');
      expect(useSchemaStore.getState().currentDatabase).toBe('db_a');
    });
    expect(onSelectTable).toHaveBeenCalledWith('users', undefined, 'db_a');
  });

  it('passes postgresql schema when opening a table under a schema node', async () => {
    connectionsState.connections = [
      {
        ...MYSQL_CONN,
        id: 'cfg-pg',
        name: 'Local PG',
        databaseType: 'postgresql',
        port: 5432,
      },
    ];
    activeConnectionsState.connections = {
      'cfg-pg': { status: 'connected', dbSessionId: 'conn-1', connectionId: 'cfg-pg' },
    };
    mockGetTables.mockImplementation((_connId: string, dbName: string) => {
      if (dbName === 'db_a') {
        return Promise.resolve([
          { name: 'users', tableType: 'table', schema: 'public', rowCount: null },
        ]);
      }
      return Promise.resolve([]);
    });

    const onSelectTable = vi.fn();
    const { findByText, queryAllByText } = render(
      <ConnectionNavigatorTree
        {...baseProps}
        activeConnectionId="cfg-pg"
        onSelectTable={onSelectTable}
      />,
    );

    await waitFor(() => findByText('db_a'));
    await waitFor(() => expect(mockGetTables).toHaveBeenCalledWith('conn-1', 'db_a'));
    fireEvent.click((await findByText('public')).closest('button')!);
    const tablesCategory = await waitFor(() => {
      const nodes = queryAllByText('schemaTree.tables');
      expect(nodes.length).toBeGreaterThan(0);
      return nodes[nodes.length - 1]!;
    });
    fireEvent.click(tablesCategory.closest('button')!);
    mockUseDatabase.mockClear();
    fireEvent.click((await findByText('users')).closest('button')!);

    await waitFor(() => {
      expect(onSelectTable).toHaveBeenCalledWith('users', 'public', 'db_a');
    });

    connectionsState.connections = [MYSQL_CONN];
    activeConnectionsState.connections = {
      'cfg-mysql': { status: 'connected', dbSessionId: 'conn-1', connectionId: 'cfg-mysql' },
    };
  });
});

describe('ConnectionNavigatorTree refresh', () => {
  it('refreshAllConnections reloads databases and expanded db tables', async () => {
    const navigatorRef = createRef<ConnectionNavigatorTreeHandle>();
    const { findByText, queryAllByText } = render(
      <ConnectionNavigatorTree {...baseProps} ref={navigatorRef} />,
    );

    await ensureDbTableVisible(findByText, queryAllByText, 'db_a', 'users');
    mockGetDatabases.mockClear();
    mockGetTables.mockClear();

    await navigatorRef.current!.refreshAllConnections();

    await waitFor(() => {
      expect(mockGetDatabases).toHaveBeenCalledWith('conn-1');
      expect(mockGetTables).toHaveBeenCalledWith('conn-1', 'db_a');
    });
  });

  it('connection context menu refresh reloads that connection without viewActions.refresh', async () => {
    const viewRefresh = vi.fn();
    const { findByText, queryAllByText } = render(
      <ConnectionNavigatorTree {...baseProps} viewActions={{ refresh: viewRefresh }} />,
    );

    await ensureDbTableVisible(findByText, queryAllByText, 'db_a', 'users');
    mockGetDatabases.mockClear();
    mockGetTables.mockClear();

    await triggerConnectionRefresh(findByText);

    await waitFor(() => {
      expect(mockGetDatabases).toHaveBeenCalledWith('conn-1');
      expect(mockGetTables).toHaveBeenCalledWith('conn-1', 'db_a');
    });
    expect(viewRefresh).not.toHaveBeenCalled();
  });

  it('database context menu refresh reloads tables for multi-db', async () => {
    const { findByText, queryAllByText } = render(<ConnectionNavigatorTree {...baseProps} />);

    await ensureDbTableVisible(findByText, queryAllByText, 'db_a', 'users');
    mockGetDatabases.mockClear();
    mockGetTables.mockClear();

    await triggerDatabaseRefresh(findByText, 'db_a');

    await waitFor(() => {
      expect(mockGetTables).toHaveBeenCalledWith('conn-1', 'db_a');
    });
    expect(mockGetDatabases).not.toHaveBeenCalled();
  });

  it('schema context menu refresh reloads tables for the schema database', async () => {
    connectionsState.connections = [
      {
        ...MYSQL_CONN,
        id: 'cfg-pg',
        name: 'Local PG',
        databaseType: 'postgresql',
        port: 5432,
      },
    ];
    activeConnectionsState.connections = {
      'cfg-pg': { status: 'connected', dbSessionId: 'conn-1', connectionId: 'cfg-pg' },
    };
    mockGetTables.mockImplementation((_connId: string, dbName: string) => {
      if (dbName === 'db_a') {
        return Promise.resolve([
          { name: 'users', tableType: 'table', schema: 'public', rowCount: null },
        ]);
      }
      return Promise.resolve([]);
    });

    const { findByText } = render(
      <ConnectionNavigatorTree {...baseProps} activeConnectionId="cfg-pg" />,
    );

    await waitFor(() => findByText('db_a'));
    await waitFor(() => expect(mockGetTables).toHaveBeenCalledWith('conn-1', 'db_a'));
    mockGetDatabases.mockClear();
    mockGetTables.mockClear();

    await triggerSchemaRefresh(findByText, 'public');

    await waitFor(() => {
      expect(mockGetTables).toHaveBeenCalledWith('conn-1', 'db_a');
    });
    expect(mockGetDatabases).not.toHaveBeenCalled();

    connectionsState.connections = [MYSQL_CONN];
    activeConnectionsState.connections = {
      'cfg-mysql': { status: 'connected', dbSessionId: 'conn-1', connectionId: 'cfg-mysql' },
    };
  });
});

describe('ConnectionNavigatorTree drop database', () => {
  it('switches away from the active database before dropping it', async () => {
    const onShowMessage = vi.fn();
    const { findByText, queryAllByText } = render(
      <ConnectionNavigatorTree {...baseProps} onShowMessage={onShowMessage} />,
    );

    await ensureDbTableVisible(findByText, queryAllByText, 'db_a', 'users');
    await waitFor(() => {
      expect(useSchemaStore.getState().currentDatabase).toBe('db_a');
    });

    mockUseDatabase.mockClear();
    mockDriverExecute.mockClear();

    await triggerDropDatabase(findByText, 'db_a');

    await waitFor(() => {
      expect(mockUseDatabase).toHaveBeenCalledWith('conn-1', 'postgres');
      expect(mockDriverExecute).toHaveBeenCalledWith({
        dbSessionId: 'conn-1',
        command: 'drop_database',
        input: { name: 'db_a' },
      });
    });
    expect(onShowMessage).not.toHaveBeenCalled();
  });

  it('shows an error when drop database fails', async () => {
    mockDriverExecute.mockRejectedValueOnce(new Error('permission denied'));
    const onShowMessage = vi.fn();
    const { findByText } = render(
      <ConnectionNavigatorTree {...baseProps} onShowMessage={onShowMessage} />,
    );

    await waitFor(() => findByText('db_a'));
    await triggerDropDatabase(findByText, 'db_a');

    await waitFor(() => {
      expect(onShowMessage).toHaveBeenCalledWith('permission denied', 'error');
    });
  });
});

describe('ConnectionNavigatorTree context menu new query', () => {
  it('sets currentDatabase to the right-clicked database before opening new query', async () => {
    const newQuery = vi.fn();
    const { findByText, queryAllByText } = render(
      <ConnectionNavigatorTree {...baseProps} viewActions={{ newQuery }} />,
    );

    await ensureDbTableVisible(findByText, queryAllByText, 'db_a', 'users');
    await ensureDbTableVisible(findByText, queryAllByText, 'db_b', 'orders');

    await waitFor(() => {
      expect(useSchemaStore.getState().currentDatabase).toBe('db_b');
    });

    await triggerContextMenuAction((await findByText('db_a')).closest('button')!, 'new-query');

    expect(useSchemaStore.getState().currentDatabase).toBe('db_a');
    expect(newQuery).toHaveBeenCalled();
  });

  it('sets currentDatabase to the schema parent database before opening new query', async () => {
    connectionsState.connections = [
      {
        ...MYSQL_CONN,
        id: 'cfg-pg',
        name: 'Local PG',
        databaseType: 'postgresql',
        port: 5432,
      },
    ];
    activeConnectionsState.connections = {
      'cfg-pg': { status: 'connected', dbSessionId: 'conn-1', connectionId: 'cfg-pg' },
    };
    mockGetTables.mockImplementation((_connId: string, dbName: string) => {
      if (dbName === 'db_a') {
        return Promise.resolve([
          { name: 'users', tableType: 'table', schema: 'public', rowCount: null },
        ]);
      }
      return Promise.resolve([]);
    });

    const newQuery = vi.fn();
    const { findByText } = render(
      <ConnectionNavigatorTree
        {...baseProps}
        activeConnectionId="cfg-pg"
        viewActions={{ newQuery }}
      />,
    );

    await waitFor(() => findByText('db_a'));
    await waitFor(() => expect(mockGetTables).toHaveBeenCalledWith('conn-1', 'db_a'));
    fireEvent.click((await findByText('public')).closest('button')!);
    await waitFor(() => {
      expect(useSchemaStore.getState().currentDatabase).toBe('db_a');
    });

    // Set a different active database to verify the fix
    useSchemaStore.setState({ currentDatabase: 'db_b' });

    await triggerContextMenuAction((await findByText('public')).closest('button')!, 'new-query');

    expect(useSchemaStore.getState().currentDatabase).toBe('db_a');
    expect(newQuery).toHaveBeenCalled();

    connectionsState.connections = [MYSQL_CONN];
    activeConnectionsState.connections = {
      'cfg-mysql': { status: 'connected', dbSessionId: 'conn-1', connectionId: 'cfg-mysql' },
    };
  });
});
