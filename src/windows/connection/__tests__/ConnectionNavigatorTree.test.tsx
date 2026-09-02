import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, cleanup, waitFor, fireEvent } from '@testing-library/react';
import { createRef } from 'react';
import {
  ConnectionNavigatorTree,
  type ConnectionNavigatorTreeHandle,
  type ConnectionNavigatorTreeProps,
} from '../ConnectionNavigatorTree';
import { useSchemaStore } from '../../../stores/schemaStore';
import type { ConnectionConfig, TableInfo } from '../../../types';
import { showWebContextMenu } from '../../../stores/contextMenuStore';

const confirmMock = vi.hoisted(() => vi.fn().mockResolvedValue(true));
const mockGetDatabaseObjects = vi.hoisted(() => vi.fn());
const mockGetDriverCommands = vi.hoisted(() => vi.fn());
const mockConnect = vi.hoisted(() => vi.fn());
const mockFetchConnections = vi.hoisted(() => vi.fn());
const mockWriteText = vi.hoisted(() => vi.fn().mockResolvedValue(undefined));
const openBackupWindowMock = vi.hoisted(() => vi.fn());

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
  openDataSyncWindow: openDataSyncWindowMock,
  openSchemaDiffWindow: openSchemaDiffWindowMock,
  openDataTransferWindow: openDataTransferWindowMock,
  openBackupWindow: openBackupWindowMock,
}));

const mockReorderConnections = vi.hoisted(() => vi.fn().mockResolvedValue(undefined));
const openDataSyncWindowMock = vi.hoisted(() => vi.fn());
const openSchemaDiffWindowMock = vi.hoisted(() => vi.fn());
const openDataTransferWindowMock = vi.hoisted(() => vi.fn());

vi.mock('../../../commands/connection', () => ({
  connectionCommands: {
    reorderConnections: (...args: unknown[]) => mockReorderConnections(...args),
  },
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
    supportsCreateUser: true,
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
      clipboardSchemes: ['mysql'],
    },
    sqlite: {
      ...sqlMulti,
      label: 'SQLite',
      quoteChar: '"',
      sqlDialect: 'sqlite',
      defaultPort: 0,
      connectionMode: 'file' as const,
      databaseFieldType: 'path' as const,
      hasMultiDatabase: false,
      supportsBackup: false,
      supportsCreateDatabase: false,
      supportsCreateSchema: false,
      supportedObjectKinds: ['function', 'procedure', 'trigger', 'sequence', 'type'] as const,
    },
    redis: {
      ...sqlMulti,
      label: 'Redis',
      shortLabel: 'RD',
      quoteChar: '',
      defaultPort: 6379,
      isKeyValue: true,
      category: 'kv' as const,
      connectionView: 'keyvalue' as const,
      databaseFieldType: 'index' as const,
      supportsTables: false,
      supportsSQL: false,
      supportsBackup: false,
      supportsCreateDatabase: false,
      supportsCreateSchema: false,
    },
    doris: {
      ...sqlMulti,
      label: 'Doris',
      namespaceEnsure: 'path-hierarchy' as const,
      hasMultiDatabase: false,
      supportsCreateSchema: false,
      supportsBackup: false,
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
    getScrollElement,
  }: {
    count: number;
    estimateSize?: (i: number) => number;
    getScrollElement?: () => unknown;
  }) => {
    // The real virtualizer resolves the scroll container during init.
    getScrollElement?.();
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
const mockExecuteQuery = vi.fn();

vi.mock('../../../commands/database', () => ({
  databaseCommands: {
    getDatabases: (...args: unknown[]) => mockGetDatabases(...args),
    getTables: (...args: unknown[]) => mockGetTables(...args),
    useDatabase: (...args: unknown[]) => mockUseDatabase(...args),
    getDatabaseObjects: (...args: unknown[]) => mockGetDatabaseObjects(...args),
  },
}));

vi.mock('../../../commands/driver', () => ({
  driverCommands: {
    execute: (...args: unknown[]) => mockDriverExecute(...args),
    getDriverCommands: (...args: unknown[]) => mockGetDriverCommands(...args),
  },
}));

vi.mock('../../../commands/query', () => ({
  queryCommands: {
    executeQuery: (...args: unknown[]) => mockExecuteQuery(...args),
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
  toggleConnectionPinned: vi.fn(),
  saveConnection: vi.fn().mockResolvedValue(undefined),
};

interface ActiveEntryFixture {
  status: 'connected' | 'connecting' | 'error' | 'idle';
  dbSessionId?: string;
  connectionId: string;
}

const activeConnectionsState = {
  connections: {
    'cfg-mysql': { status: 'connected', dbSessionId: 'conn-1', connectionId: 'cfg-mysql' },
  } as Record<string, ActiveEntryFixture>,
};

vi.mock('../../../stores/connectionStore', () => {
  const useConnectionStore = Object.assign(
    (sel: (s: typeof connectionsState) => unknown) => sel(connectionsState),
    {
      getState: () => ({ ...connectionsState, fetchConnections: mockFetchConnections }),
    },
  );
  return {
    useConnectionStore,
    groupConnections: (connections: ConnectionConfig[], groups: string[], _query: string) => [
      { group: '', connections },
    ],
    groupConnectionsWithPinnedSection: (connections: ConnectionConfig[], groups: string[]) => {
      if (connections.length === 0) return [];
      const sections = groups.map((g) => ({
        group: g,
        connections: connections.filter((c) => (c.group ?? '') === g),
      }));
      const recent = connections.filter((connection) => connection.lastConnectedAt);
      if (recent.length > 0) {
        sections.unshift({ group: '__recent__', connections: recent });
      }
      const known = new Set(groups);
      for (const c of connections) {
        const g = c.group ?? '';
        if (!known.has(g)) {
          sections.push({
            group: g,
            connections: connections.filter((x) => (x.group ?? '') === g),
          });
          known.add(g);
        }
      }
      return sections;
    },
    PINNED_GROUP_KEY: '__pinned__',
  };
});

vi.mock('../../../stores/activeConnectionStore', () => ({
  useActiveConnectionStore: Object.assign(
    (sel: (s: typeof activeConnectionsState & { connect: () => void }) => unknown) =>
      sel({ ...activeConnectionsState, connect: mockConnect }),
    {
      getState: () => ({ ...activeConnectionsState, connect: mockConnect }),
    },
  ),
}));

const panelStoreState = {
  pendingQueryHistoryConnectionId: null as string | null,
};
const mockSetPendingQueryHistory = vi.fn((id: string | null) => {
  panelStoreState.pendingQueryHistoryConnectionId = id;
});
vi.mock('../../../stores/panelStore', () => ({
  usePanelStore: Object.assign(
    (sel: (s: typeof panelStoreState) => unknown) => sel(panelStoreState),
    {
      getState: () => ({
        pendingQueryHistoryConnectionId: panelStoreState.pendingQueryHistoryConnectionId,
        setPendingQueryHistory: mockSetPendingQueryHistory,
      }),
    },
  ),
  nextPanelId: (prefix: string) => `panel-${prefix}-test`,
}));

async function ensureDbTableVisible(
  findByText: (text: string) => Promise<HTMLElement>,
  queryAllByText: (text: string) => HTMLElement[],
  dbName: string,
  tableName: string,
  dbSessionId = 'conn-1',
) {
  await waitFor(() => findByText(dbName));

  if (queryAllByText(tableName).length > 0) return;

  fireEvent.click((await findByText(dbName)).closest('button')!);
  await waitFor(() => {
    expect(mockGetTables).toHaveBeenCalledWith(dbSessionId, dbName);
    expect(queryAllByText(tableName).length).toBeGreaterThan(0);
  });
}

/** Activate the local SQL context on `dbName` by opening one of its tables. */
async function activateDatabaseContext(
  findByText: (text: string) => Promise<HTMLElement>,
  dbName: string,
  tableName: string,
) {
  fireEvent.click((await findByText(tableName)).closest('button')!);
  await waitFor(() => {
    expect(useSchemaStore.getState().currentDatabase).toBe(dbName);
  });
}

/** Expand a PG database → schema path until `tableName` is visible. */
async function ensurePgSchemaTableVisible(
  findByText: (text: string) => Promise<HTMLElement>,
  queryAllByText: (text: string) => HTMLElement[],
  dbName: string,
  schemaName: string,
  tableName: string,
  dbSessionId = 'conn-pg',
) {
  await waitFor(() => findByText(dbName));
  fireEvent.click((await findByText(dbName)).closest('button')!);
  await waitFor(() => {
    expect(mockGetTables).toHaveBeenCalledWith(dbSessionId, dbName);
  });
  fireEvent.click((await findByText(schemaName)).closest('button')!);
  await waitFor(() => {
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

async function triggerConnectionRefresh(
  findByText: (text: string) => Promise<HTMLElement>,
  connName = 'Local MySQL',
) {
  const connLabel = await findByText(connName);
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

// ── Shared helpers for the extended suites ──────────────────────

function makeConn(
  overrides: Partial<ConnectionConfig> & { id: string; name: string },
): ConnectionConfig {
  return {
    databaseType: 'mysql',
    host: '127.0.0.1',
    port: 3306,
    sslMode: 'disable',
    group: '',
    ...overrides,
  };
}

type SessionSchemaPatch = {
  currentDatabase?: string | null;
  databases?: string[];
  tables?: TableInfo[];
  views?: TableInfo[];
  schemaNames?: string[];
  namespaceTree?: Record<string, unknown>;
  loadedPaths?: Set<string>;
  pathItems?: Record<string, TableInfo[]>;
  loading?: boolean;
};

const EMPTY_SESSION_SCHEMA = {
  currentDatabase: null,
  databases: [],
  databaseType: null,
  isMultiDatabase: false,
  tables: [],
  views: [],
  schemaNames: [],
  columnMap: {},
  namespaceTree: {},
  loadedPaths: new Set<string>(),
  pathItems: {},
  pathAliases: {},
  namespaceOwnedByPlugin: false,
  schemaEpoch: 0,
  expanded: new Set<string>(),
  selectedId: null,
  loading: false,
  ensuringCount: 0,
  error: null,
  columnInflight: new Set<string>(),
};

/** Patch (or create) one session's schema-cache entry in the real store. */
function seedSessionSchema(dbSessionId: string, patch: SessionSchemaPatch): void {
  useSchemaStore.setState((state) => {
    const base = state.schemas.get(dbSessionId) ?? {
      ...EMPTY_SESSION_SCHEMA,
      loadedPaths: new Set<string>(),
      expanded: new Set<string>(),
      columnInflight: new Set<string>(),
    };
    const next = new Map(state.schemas);
    next.set(dbSessionId, { ...base, ...patch });
    return { schemas: next };
  });
}

async function settleSessionLoad(dbSessionId: string) {
  await waitFor(() => {
    expect(useSchemaStore.getState().schemas.get(dbSessionId)).toBeTruthy();
  });
}

function connRow(container: HTMLElement, name: string): HTMLElement {
  const row = container.querySelector<HTMLElement>(`[data-conn-name="${name}"]`);
  if (!row) throw new Error(`connection row not found: ${name}`);
  return row;
}

afterEach(() => {
  cleanup();
  useSchemaStore.getState().reset();
  vi.clearAllMocks();
  confirmMock.mockResolvedValue(true);
  // Deterministic fixtures for the next test regardless of how the previous
  // one mutated the module-level store mocks.
  connectionsState.connections = [MYSQL_CONN];
  connectionsState.groups = [''];
  activeConnectionsState.connections = {
    'cfg-mysql': { status: 'connected', dbSessionId: 'conn-1', connectionId: 'cfg-mysql' },
  };
});

beforeEach(() => {
  panelStoreState.pendingQueryHistoryConnectionId = null;
  mockSetPendingQueryHistory.mockClear();
  Object.defineProperty(window.navigator, 'clipboard', {
    value: { writeText: mockWriteText },
    configurable: true,
  });
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
  mockExecuteQuery.mockResolvedValue({});
  mockGetDatabaseObjects.mockResolvedValue([]);
  mockGetDriverCommands.mockResolvedValue([]);
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

    fireEvent.click((await findByText('users')).closest('button')!);

    await waitFor(() => {
      // F1: no use_database IPC — activation only moves the local context.
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
  it('pins the backend session to fallback before dropping the active database', async () => {
    const onShowMessage = vi.fn();
    const { findByText, queryAllByText } = render(
      <ConnectionNavigatorTree {...baseProps} onShowMessage={onShowMessage} />,
    );

    await ensureDbTableVisible(findByText, queryAllByText, 'db_a', 'users');
    await waitFor(() => {
      expect(useSchemaStore.getState().currentDatabase).toBe('db_a');
    });

    await triggerDropDatabase(findByText, 'db_a');

    await waitFor(() => {
      expect(mockGetTables).toHaveBeenCalledWith('conn-1', 'postgres');
      expect(useSchemaStore.getState().currentDatabase).toBe('postgres');
      expect(mockDriverExecute).toHaveBeenCalledWith({
        dbSessionId: 'conn-1',
        command: 'drop_database',
        input: { name: 'db_a' },
      });
    });
    expect(onShowMessage).not.toHaveBeenCalled();
  });

  it('pins backend session away when dropping a non-active database while session is pinned there', async () => {
    const onShowMessage = vi.fn();
    const { findByText, queryAllByText } = render(
      <ConnectionNavigatorTree {...baseProps} onShowMessage={onShowMessage} />,
    );

    await ensureDbTableVisible(findByText, queryAllByText, 'db_a', 'users');
    await ensureDbTableVisible(findByText, queryAllByText, 'db_b', 'orders');
    await waitFor(() => {
      expect(useSchemaStore.getState().currentDatabase).toBe('db_b');
    });

    mockGetTables.mockClear();

    await triggerDropDatabase(findByText, 'db_a');

    await waitFor(() => {
      expect(mockGetTables).toHaveBeenCalledWith('conn-1', 'postgres');
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

// ── Extended coverage suites ─────────────────────────────────────

interface MenuLeaf {
  id?: string;
  action?: () => void;
  items?: MenuLeaf[];
}

function lastMenuItems(): MenuLeaf[] {
  const call = vi.mocked(showWebContextMenu).mock.calls.at(-1)?.[0];
  return (call ?? []) as unknown as MenuLeaf[];
}

/** Recursively find a menu item by id, searching through submenus. */
function findMenuItem(items: MenuLeaf[], id: string): MenuLeaf | undefined {
  for (const item of items) {
    if (item.id === id) return item;
    if (item.items) {
      const found = findMenuItem(item.items, id);
      if (found) return found;
    }
  }
  return undefined;
}

async function openMenuAndPick(element: HTMLElement, actionId: string): Promise<void> {
  fireEvent.contextMenu(element);
  await waitFor(() => {
    expect(findMenuItem(lastMenuItems(), actionId)).toBeDefined();
  });
  findMenuItem(lastMenuItems(), actionId)?.action?.();
}

function searchInput(container: HTMLElement): HTMLInputElement {
  const input = container.querySelector<HTMLInputElement>('input[type="text"]');
  if (!input) throw new Error('search input not found');
  return input;
}

function categoryButton(container: HTMLElement, catId: string): HTMLElement {
  const nodes = container.querySelectorAll<HTMLElement>(`[data-cat-id="${catId}"]`);
  const btn = nodes[nodes.length - 1]?.closest('button');
  if (!btn) throw new Error(`category button not found: ${catId}`);
  return btn;
}

describe('ConnectionNavigatorTree toolbar and empty states', () => {
  it('shows the empty placeholder and creates the first connection from it', async () => {
    connectionsState.connections = [];
    const onNewConnection = vi.fn();
    const view = render(
      <ConnectionNavigatorTree {...baseProps} onNewConnection={onNewConnection} />,
    );
    await view.findByText('main.noConnections');
    fireEvent.click(view.getByText('main.createFirst'));
    expect(onNewConnection).toHaveBeenCalledTimes(1);
  });

  it('wires optional toolbar buttons and collapse-all clears every expansion', async () => {
    const onExportConnections = vi.fn();
    const onImportConnections = vi.fn();
    const onRefresh = vi.fn();
    const onCollapseSidebar = vi.fn();
    const newConnectionSpy = baseProps.onNewConnection as ReturnType<typeof vi.fn>;
    const { container, findByText, queryAllByText, queryByText } = render(
      <ConnectionNavigatorTree
        {...baseProps}
        onExportConnections={onExportConnections}
        onImportConnections={onImportConnections}
        onRefresh={onRefresh}
        onCollapseSidebar={onCollapseSidebar}
      />,
    );

    await ensureDbTableVisible(findByText, queryAllByText, 'db_a', 'users');

    fireEvent.click(container.querySelector('button[title="common.exportConnections"]')!);
    fireEvent.click(container.querySelector('button[title="common.importConnections"]')!);
    fireEvent.click(container.querySelector('button[title="connWin.refresh"]')!);
    fireEvent.click(container.querySelector('button[title="connWin.collapseSidebar"]')!);
    expect(onExportConnections).toHaveBeenCalledTimes(1);
    expect(onImportConnections).toHaveBeenCalledTimes(1);
    expect(onRefresh).toHaveBeenCalledTimes(1);
    expect(onCollapseSidebar).toHaveBeenCalledTimes(1);

    expect(queryByText('users')).not.toBeNull();
    fireEvent.click(container.querySelector('button[title="connWin.collapseAll"]')!);
    await waitFor(() => {
      expect(queryByText('db_a')).toBeNull();
      expect(queryByText('users')).toBeNull();
    });

    // New-connection toolbar button still dispatches.
    newConnectionSpy.mockClear();
    fireEvent.click(screen.getByTestId('new-connection-button'));
    expect(newConnectionSpy).toHaveBeenCalledTimes(1);
  });

  it('renders status dots for connecting, connected and error rows', async () => {
    connectionsState.connections = [
      MYSQL_CONN,
      makeConn({ id: 'cfg-c', name: 'Connecting Conn' }),
      makeConn({ id: 'cfg-e', name: 'Error Conn' }),
      makeConn({ id: 'cfg-i', name: 'Idle Conn' }),
    ];
    activeConnectionsState.connections = {
      'cfg-mysql': { status: 'connected', dbSessionId: 'conn-1', connectionId: 'cfg-mysql' },
      'cfg-c': { status: 'connecting', connectionId: 'cfg-c' },
      'cfg-e': { status: 'error', connectionId: 'cfg-e' },
    };
    const { container } = render(<ConnectionNavigatorTree {...baseProps} />);
    await waitFor(() => {
      expect(container.querySelector('[title="conn.connecting"]')).not.toBeNull();
    });
    expect(container.querySelector('[title="conn.connected"]')).not.toBeNull();
    expect(container.querySelector('[title="conn.failed"]')).not.toBeNull();
    expect(container.querySelectorAll('[data-conn-name="Idle Conn"] [title]').length).toBe(0);
  });
});

describe('ConnectionNavigatorTree connection row interactions', () => {
  it('selects a connection on single click', async () => {
    const onSelectConnection = vi.fn();
    const { container } = render(
      <ConnectionNavigatorTree {...baseProps} onSelectConnection={onSelectConnection} />,
    );
    await waitFor(() => connRow(container, 'Local MySQL'));
    fireEvent.click(connRow(container, 'Local MySQL'));
    fireEvent.click(connRow(container, 'Local MySQL'));
    expect(onSelectConnection).toHaveBeenCalledWith('cfg-mysql');
  });

  it('double-click connects idle rows but is a no-op for connecting/connected rows', async () => {
    connectionsState.connections = [
      MYSQL_CONN,
      makeConn({ id: 'cfg-idle', name: 'Idle Conn' }),
      makeConn({ id: 'cfg-busy', name: 'Busy Conn' }),
    ];
    activeConnectionsState.connections = {
      'cfg-mysql': { status: 'connected', dbSessionId: 'conn-1', connectionId: 'cfg-mysql' },
      'cfg-busy': { status: 'connecting', connectionId: 'cfg-busy' },
    };
    const { container } = render(<ConnectionNavigatorTree {...baseProps} />);
    await waitFor(() => connRow(container, 'Idle Conn'));

    fireEvent.doubleClick(connRow(container, 'Idle Conn'));
    expect(mockConnect).toHaveBeenCalledTimes(1);
    expect(mockConnect.mock.calls[0]?.[0]).toMatchObject({ id: 'cfg-idle' });

    fireEvent.doubleClick(connRow(container, 'Busy Conn'));
    fireEvent.doubleClick(connRow(container, 'Local MySQL'));
    expect(mockConnect).toHaveBeenCalledTimes(1);
  });

  it('chevron toggles expansion for connected rows and connects idle ones', async () => {
    const view = render(<ConnectionNavigatorTree {...baseProps} />);
    await view.findByText('db_a');
    const chevron = connRow(view.container, 'Local MySQL').querySelector('button')!;
    fireEvent.click(chevron);
    await waitFor(() => expect(view.queryByText('db_a')).toBeNull());
    fireEvent.click(chevron);
    await waitFor(() => expect(view.queryByText('db_a')).not.toBeNull());

    connectionsState.connections = [MYSQL_CONN, makeConn({ id: 'cfg-idle', name: 'Idle Conn' })];
    activeConnectionsState.connections = {
      'cfg-mysql': { status: 'connected', dbSessionId: 'conn-1', connectionId: 'cfg-mysql' },
    };
    view.rerender(<ConnectionNavigatorTree {...baseProps} />);
    await waitFor(() => connRow(view.container, 'Idle Conn'));
    const idleChevron = connRow(view.container, 'Idle Conn').querySelector('button')!;
    fireEvent.click(idleChevron);
    expect(mockConnect).toHaveBeenCalledWith(expect.objectContaining({ id: 'cfg-idle' }));
  });

  it('scopes expansion to the clicked section and keeps only one connection expanded', async () => {
    const recent = makeConn({
      id: 'cfg-recent',
      name: 'Recent Conn',
      group: 'Group A',
      lastConnectedAt: '2026-08-31T10:00:00Z',
    });
    const other = makeConn({ id: 'cfg-other', name: 'Other Conn', group: 'Group B' });
    connectionsState.connections = [recent, other];
    connectionsState.groups = ['Group A', 'Group B'];
    activeConnectionsState.connections = {
      'cfg-recent': {
        status: 'connected',
        dbSessionId: 'session-recent',
        connectionId: 'cfg-recent',
      },
      'cfg-other': {
        status: 'connected',
        dbSessionId: 'session-other',
        connectionId: 'cfg-other',
      },
    };

    const { container } = render(
      <ConnectionNavigatorTree {...baseProps} activeConnectionId={null} />,
    );
    await waitFor(() => {
      expect(container.querySelector('[data-conn-group="__recent__"]')).not.toBeNull();
      expect(container.querySelector('[data-conn-group="Group A"]')).not.toBeNull();
    });

    const expansion = (group: string, name: string) =>
      container.querySelector<HTMLButtonElement>(
        `[data-conn-group="${group}"][data-conn-name="${name}"] button`,
      );
    const recentShortcut = expansion('__recent__', 'Recent Conn')!;
    const groupedRecent = expansion('Group A', 'Recent Conn')!;
    const groupedOther = expansion('Group B', 'Other Conn')!;

    expect(recentShortcut.getAttribute('aria-expanded')).toBe('true');
    expect(groupedRecent.getAttribute('aria-expanded')).toBe('false');
    expect(groupedOther.getAttribute('aria-expanded')).toBe('false');

    fireEvent.click(expansion('Group A', 'Recent Conn')!);
    await waitFor(() => {
      expect(expansion('Group A', 'Recent Conn')?.getAttribute('aria-expanded')).toBe('true');
      expect(expansion('__recent__', 'Recent Conn')?.getAttribute('aria-expanded')).toBe('false');
    });

    fireEvent.click(expansion('Group B', 'Other Conn')!);
    await waitFor(() => {
      expect(expansion('Group B', 'Other Conn')?.getAttribute('aria-expanded')).toBe('true');
      expect(expansion('Group A', 'Recent Conn')?.getAttribute('aria-expanded')).toBe('false');
      expect(expansion('__recent__', 'Recent Conn')?.getAttribute('aria-expanded')).toBe('false');
    });
  });

  it('keeps a slow selected connection pending and expands that exact row after success', async () => {
    const recent = makeConn({
      id: 'cfg-recent',
      name: 'Recent Conn',
      group: 'Group A',
      lastConnectedAt: '2026-08-31T10:00:00Z',
    });
    const slow = makeConn({ id: 'cfg-slow', name: 'Slow Conn', group: 'Group B' });
    connectionsState.connections = [recent, slow];
    connectionsState.groups = ['Group A', 'Group B'];
    activeConnectionsState.connections = {
      'cfg-recent': {
        status: 'connected',
        dbSessionId: 'session-recent',
        connectionId: 'cfg-recent',
      },
    };

    const onSelectConnection = vi.fn();
    const view = render(
      <ConnectionNavigatorTree
        {...baseProps}
        activeConnectionId={null}
        onSelectConnection={onSelectConnection}
      />,
    );
    const expansion = (group: string, name: string) =>
      view.container.querySelector<HTMLButtonElement>(
        `[data-conn-group="${group}"][data-conn-name="${name}"] button`,
      );
    const row = (group: string, name: string) =>
      view.container.querySelector<HTMLElement>(
        `[data-conn-group="${group}"][data-conn-name="${name}"]`,
      );

    await waitFor(() => expect(expansion('__recent__', 'Recent Conn')).not.toBeNull());
    await waitFor(() =>
      expect(expansion('__recent__', 'Recent Conn')?.getAttribute('aria-expanded')).toBe('true'),
    );

    fireEvent.click(expansion('__recent__', 'Recent Conn')!);
    await waitFor(() =>
      expect(expansion('__recent__', 'Recent Conn')?.getAttribute('aria-expanded')).toBe('false'),
    );

    fireEvent.click(row('Group B', 'Slow Conn')!);
    expect(onSelectConnection).toHaveBeenCalledWith('cfg-slow');

    activeConnectionsState.connections = {
      ...activeConnectionsState.connections,
      'cfg-slow': { status: 'connecting', connectionId: 'cfg-slow' },
    };
    view.rerender(
      <ConnectionNavigatorTree
        {...baseProps}
        activeConnectionId="cfg-slow"
        onSelectConnection={onSelectConnection}
      />,
    );
    await waitFor(() => {
      expect(expansion('__recent__', 'Recent Conn')?.getAttribute('aria-expanded')).toBe('false');
      expect(expansion('Group A', 'Recent Conn')?.getAttribute('aria-expanded')).toBe('false');
      expect(expansion('Group B', 'Slow Conn')?.getAttribute('aria-expanded')).toBe('true');
      expect(view.container.textContent).toContain('common.loading');
    });

    activeConnectionsState.connections = {
      ...activeConnectionsState.connections,
      'cfg-slow': {
        status: 'connected',
        dbSessionId: 'session-slow',
        connectionId: 'cfg-slow',
      },
    };
    view.rerender(
      <ConnectionNavigatorTree
        {...baseProps}
        activeConnectionId="cfg-slow"
        onSelectConnection={onSelectConnection}
      />,
    );
    await waitFor(() => {
      expect(expansion('Group B', 'Slow Conn')?.getAttribute('aria-expanded')).toBe('true');
      expect(expansion('__recent__', 'Recent Conn')?.getAttribute('aria-expanded')).toBe('false');
      expect(expansion('Group A', 'Recent Conn')?.getAttribute('aria-expanded')).toBe('false');
    });
  });
});

describe('ConnectionNavigatorTree drag & drop reordering', () => {
  function dataTransferStub() {
    return { setData: vi.fn(), effectAllowed: '', dropEffect: '' };
  }

  it('reorders connections after a valid drop and refetches the list', async () => {
    connectionsState.connections = [
      MYSQL_CONN,
      makeConn({ id: 'cfg-b', name: 'Conn B' }),
      makeConn({ id: 'cfg-c', name: 'Conn C' }),
    ];
    const { container } = render(<ConnectionNavigatorTree {...baseProps} />);
    await waitFor(() => connRow(container, 'Conn C'));

    const dt = dataTransferStub();
    fireEvent.dragStart(connRow(container, 'Local MySQL'), { dataTransfer: dt });
    expect(dt.setData).toHaveBeenCalledWith('text/plain', 'cfg-mysql');

    // clientY below the (zero-sized jsdom) row midpoint → insert after target.
    fireEvent.dragOver(connRow(container, 'Conn C'), { dataTransfer: dt, clientY: 10 });
    fireEvent.drop(connRow(container, 'Conn C'), { dataTransfer: dt });

    await waitFor(() => expect(mockFetchConnections).toHaveBeenCalled());
    expect(mockReorderConnections).toHaveBeenCalledWith(['cfg-b', 'cfg-c', 'cfg-mysql']);
  });

  it('ignores drops without an active drag target or with an unchanged order', async () => {
    connectionsState.connections = [MYSQL_CONN, makeConn({ id: 'cfg-b', name: 'Conn B' })];
    const { container } = render(<ConnectionNavigatorTree {...baseProps} />);
    await waitFor(() => connRow(container, 'Conn B'));

    const dt = dataTransferStub();
    // Dropping without ever hovering another row clears the drag state.
    fireEvent.dragStart(connRow(container, 'Local MySQL'), { dataTransfer: dt });
    fireEvent.drop(connRow(container, 'Local MySQL'), { dataTransfer: dt });
    expect(mockReorderConnections).not.toHaveBeenCalled();

    // Hovering the dragged row itself resets the drop indicator.
    fireEvent.dragOver(connRow(container, 'Local MySQL'), { dataTransfer: dt, clientY: 5 });
    // dragLeave clears the pending target so the next drop is ignored too.
    fireEvent.dragOver(connRow(container, 'Conn B'), { dataTransfer: dt, clientY: -10 });
    fireEvent.dragLeave(connRow(container, 'Conn B'));
    fireEvent.drop(connRow(container, 'Conn B'), { dataTransfer: dt });
    fireEvent.dragEnd(connRow(container, 'Conn B'));
    expect(mockReorderConnections).not.toHaveBeenCalled();
    expect(mockFetchConnections).not.toHaveBeenCalled();
  });
});

describe('ConnectionNavigatorTree search filtering', () => {
  it('debounce-filters by connection name and restores rows when cleared', async () => {
    connectionsState.connections = [
      MYSQL_CONN,
      makeConn({
        id: 'cfg-other',
        name: 'Other Pg',
        databaseType: 'postgresql',
        port: 5432,
      }),
    ];
    const { container } = render(<ConnectionNavigatorTree {...baseProps} />);
    await waitFor(() => connRow(container, 'Other Pg'));

    fireEvent.change(searchInput(container), { target: { value: 'local' } });
    await waitFor(() => {
      expect(container.querySelector('[data-conn-name="Other Pg"]')).toBeNull();
    });
    expect(container.querySelector('[data-conn-name="Local MySQL"]')).not.toBeNull();

    fireEvent.change(searchInput(container), { target: { value: '' } });
    await waitFor(() => {
      expect(container.querySelector('[data-conn-name="Other Pg"]')).not.toBeNull();
    });
  });

  it('orders search results globally and exposes the match context', async () => {
    connectionsState.connections = [
      makeConn({ id: 'cfg-exact', name: 'prod' }),
      makeConn({ id: 'cfg-prefix', name: 'prod reporting', pinned: true }),
      makeConn({ id: 'cfg-host', name: 'Other', host: 'prod.example.com' }),
    ];
    const { container } = render(<ConnectionNavigatorTree {...baseProps} />);
    await waitFor(() => connRow(container, 'Other'));

    fireEvent.change(searchInput(container), { target: { value: 'prod' } });
    await waitFor(() => {
      const names = [...container.querySelectorAll<HTMLElement>('[data-conn-name]')].map(
        (row) => row.dataset.connName,
      );
      expect(names).toEqual(['prod', 'prod reporting', 'Other']);
      expect(connRow(container, 'prod').dataset.searchMatchReason).toBe('name');
    });

    const exact = connRow(container, 'prod');
    expect(exact.dataset.searchMatchReason).toBe('name');
    expect(exact.dataset.searchMatchContext).toBe('prod');
    expect(connRow(container, 'Other').dataset.searchMatchReason).toBe('host');
    expect(connRow(container, 'Other').dataset.searchMatchContext).toBe('prod.example.com');
  });

  it('matches cached per-database tables deep in local state', async () => {
    const { container, findByText, queryAllByText, queryByText } = render(
      <ConnectionNavigatorTree {...baseProps} />,
    );
    await ensureDbTableVisible(findByText, queryAllByText, 'db_b', 'orders');

    fireEvent.change(searchInput(container), { target: { value: 'orders' } });
    // Wait for the debounce to prune non-matching databases.
    await waitFor(() => {
      expect(queryByText('db_a')).toBeNull();
    });
    expect(queryByText('db_b')).not.toBeNull();
    expect(container.querySelector('[data-conn-name="Local MySQL"]')).not.toBeNull();
    // Query forces the matching branch open and filters its table rows.
    expect(queryByText('orders')).not.toBeNull();
  });

  it('deep-matches views, schema names and cached path items from the schema store', async () => {
    connectionsState.connections = [
      makeConn({ id: 'cfg-pg', name: 'Deep PG', databaseType: 'postgresql', port: 5432 }),
    ];
    activeConnectionsState.connections = {
      'cfg-pg': { status: 'connected', dbSessionId: 'conn-pg', connectionId: 'cfg-pg' },
    };
    mockGetDatabases.mockResolvedValue(['alpha_db']);
    const { container, findByText } = render(
      <ConnectionNavigatorTree {...baseProps} activeConnectionId="cfg-pg" />,
    );
    await findByText('alpha_db');
    await settleSessionLoad('conn-pg');
    seedSessionSchema('conn-pg', {
      databases: ['alpha_db'],
      currentDatabase: 'alpha_db',
      tables: [{ name: 't_plain', tableType: 'table', schema: 'schemax' }],
      views: [{ name: 'v_secret', tableType: 'view' }],
      pathItems: { p: [{ name: 'pathitemz', tableType: 'table' }] },
    });

    fireEvent.change(searchInput(container), { target: { value: 'v_secret' } });
    await waitFor(() => {
      expect(container.querySelector('[data-conn-name="Deep PG"]')).not.toBeNull();
    });

    fireEvent.change(searchInput(container), { target: { value: 'schemax' } });
    await waitFor(() => {
      expect(container.querySelector('[data-conn-name="Deep PG"]')).not.toBeNull();
    });

    fireEvent.change(searchInput(container), { target: { value: 'pathitemz' } });
    await waitFor(() => {
      expect(container.querySelector('[data-conn-name="Deep PG"]')).not.toBeNull();
    });

    fireEvent.change(searchInput(container), { target: { value: 'zzz_nothing' } });
    await waitFor(() => {
      expect(container.querySelector('[data-conn-name="Deep PG"]')).toBeNull();
    });
  });
});

describe('ConnectionNavigatorTree standard single-db trees', () => {
  it('renders tables and views for the auto-expanded sqlite database', async () => {
    const { container } = await renderWithSqlite(
      [
        { name: 'settings', tableType: 'table', schema: null },
        { name: 'v_app', tableType: 'view', schema: null },
        { name: 'idx_log', tableType: 'systemTable', schema: null },
      ],
      {},
      {},
    );

    const dbNode = await waitFor(() => {
      const el = container.querySelector<HTMLElement>('[data-tree-node="db"]');
      expect(el).not.toBeNull();
      return el!;
    });
    expect(dbNode.getAttribute('data-db-name')).toBe('/data/app.db');

    await waitFor(() => {
      expect(container.querySelector('[data-item-name="settings"]')).not.toBeNull();
    });
    expect(container.querySelector('[data-item-name="idx_log"]')).not.toBeNull();

    // Views live in their own collapsed category — expand it.
    fireEvent.click(categoryButton(container, 'views'));
    await waitFor(() => {
      expect(
        container.querySelector('[data-tree-node="view"][data-item-name="v_app"]'),
      ).not.toBeNull();
    });

    fireEvent.contextMenu(container.querySelector('[data-item-name="settings"]')!);
    await waitFor(() => {
      expect(lastMenuItems().some((i) => i.id === 'drop')).toBe(true);
    });
  });

  it('groups by schema, sorts names and hides system schemas via object filter', async () => {
    const conn = makeConn({
      id: 'cfg-sql',
      name: 'SQLite Conn',
      databaseType: 'sqlite',
      database: '/data/app.db',
      options: { objectFilter: { hideSystemSchemas: true } },
    });
    connectionsState.connections = [conn];
    activeConnectionsState.connections = {
      'cfg-sql': { status: 'connected', dbSessionId: 'conn-sql', connectionId: 'cfg-sql' },
    };
    mockGetDatabases.mockResolvedValue(['/data/app.db']);
    mockGetTables.mockResolvedValue([
      { name: '', tableType: 'table', schema: 'temp' }, // nameless rows are dropped
      { name: 'settings', tableType: 'table', schema: null },
      { name: 'cache', tableType: 'table', schema: 'temp' },
      { name: 'pg_internal', tableType: 'table', schema: 'information_schema' },
    ] as TableInfo[]);

    const { container, findByText } = render(
      <ConnectionNavigatorTree {...baseProps} activeConnectionId="cfg-sql" />,
    );
    await findByText('/data/app.db');
    await settleSessionLoad('conn-sql');

    await waitFor(() => {
      expect(container.querySelector('[data-schema-name="temp"]')).not.toBeNull();
    });
    const schemaNames = [...container.querySelectorAll('[data-schema-name]')].map((el) =>
      el.getAttribute('data-schema-name'),
    );
    expect(schemaNames).toContain('');
    expect(schemaNames).not.toContain('information_schema');

    // Expand the default ('' → common.default label) schema section.
    fireEvent.click(container.querySelector('[data-schema-name=""]')!.closest('button')!);
    await waitFor(() => {
      expect(categoryButton(container, 'tables')).toBeTruthy();
    });
  });

  it('lazy-loads object categories and dispatches openObject by kind', async () => {
    const openObject = vi.fn();
    const { container, findByText } = await renderWithSqlite(
      [{ name: 'settings', tableType: 'table', schema: null }],
      {},
      { viewActions: { openObject } },
    );
    await findByText('settings');

    // Category ids are the singular object kinds rendered by the component.
    mockGetDatabaseObjects.mockImplementation((_c: string, catId: string) => {
      if (catId === 'function') {
        return Promise.resolve([
          { name: 'fn_calc', kind: 'function' },
          { name: 'fn_min', kind: 'function' },
        ]);
      }
      if (catId === 'trigger') return Promise.resolve([{ name: 'tg_del', kind: 'trigger' }]);
      if (catId === 'sequence') return Promise.resolve([{ name: 'sq_next', kind: 'sequence' }]);
      return Promise.resolve([]);
    });

    fireEvent.click(categoryButton(container, 'function'));
    await waitFor(() => {
      expect(container.querySelector('[data-item-name="fn_calc"]')).not.toBeNull();
    });
    expect(categoryButton(container, 'function').textContent).toContain('2');

    fireEvent.click(container.querySelector('[data-item-name="fn_calc"]')!);
    await waitFor(() => {
      expect(openObject).toHaveBeenCalledWith('function', 'fn_calc', undefined);
    });

    fireEvent.click(categoryButton(container, 'trigger'));
    await waitFor(() => {
      expect(container.querySelector('[data-item-name="tg_del"]')).not.toBeNull();
    });
    fireEvent.click(container.querySelector('[data-item-name="tg_del"]')!);
    await waitFor(() => {
      expect(openObject).toHaveBeenCalledWith('trigger', 'tg_del', undefined);
    });

    fireEvent.click(categoryButton(container, 'sequence'));
    await waitFor(() => {
      expect(container.querySelector('[data-item-name="sq_next"]')).not.toBeNull();
    });
    openObject.mockClear();
    fireEvent.click(container.querySelector('[data-item-name="sq_next"]')!);
    expect(openObject).not.toHaveBeenCalled();

    // Object context menu copies the object name.
    await openMenuAndPick(container.querySelector('[data-item-name="fn_calc"]')!, 'copy-name');
    expect(mockWriteText).toHaveBeenCalledWith('fn_calc');
  });

  it('caches an empty list when an object category fails to refresh', async () => {
    const { container, findByText } = await renderWithSqlite(
      [{ name: 'settings', tableType: 'table', schema: null }],
      {},
      {},
    );
    await findByText('settings');
    mockGetDatabaseObjects.mockResolvedValue([{ name: 'pr_x', kind: 'procedure' }]);
    fireEvent.click(categoryButton(container, 'procedure'));
    await waitFor(() => {
      expect(container.querySelector('[data-item-name="pr_x"]')).not.toBeNull();
    });
    expect(categoryButton(container, 'procedure').textContent).toContain('1');

    // Category context menu → refresh → backend failure falls back to [].
    mockGetDatabaseObjects.mockRejectedValueOnce(new Error('boom'));
    await openMenuAndPick(categoryButton(container, 'procedure'), 'refresh');
    await waitFor(() => {
      expect(categoryButton(container, 'procedure').textContent).toContain('0');
    });
    expect(container.querySelector('[data-item-name="pr_x"]')).toBeNull();
  });

  it('activates the clicked database in local state when nothing is cached', async () => {
    const onSelectTable = vi.fn();
    const { container, findByText } = await renderWithSqlite(
      [{ name: 'settings', tableType: 'table', schema: null }],
      {},
      { onSelectTable },
    );
    await findByText('settings');
    seedSessionSchema('conn-sql', { currentDatabase: 'other.db' });

    fireEvent.click(container.querySelector('[data-item-name="settings"]')!);

    await waitFor(() => {
      expect(useSchemaStore.getState().schemas.get('conn-sql')?.currentDatabase).toBe(
        '/data/app.db',
      );
    });
    expect(onSelectTable).toHaveBeenCalledWith('settings', undefined, '/data/app.db');
  });

  it('refresh paths reload expanded categories and single-db tables', async () => {
    const { container, findByText } = await renderWithSqlite(
      [{ name: 'settings', tableType: 'table', schema: null }],
      {},
      {},
    );
    await findByText('settings');

    // Expand procedures so refresh has an expanded object category to reload.
    mockGetDatabaseObjects.mockResolvedValue([{ name: 'pr_y', kind: 'procedure' }]);
    fireEvent.click(categoryButton(container, 'procedure'));
    await waitFor(() => {
      expect(container.querySelector('[data-item-name="pr_y"]')).not.toBeNull();
    });

    // Category context menu → refresh → backend reload of that category.
    mockGetDatabaseObjects.mockClear();
    mockGetTables.mockClear();
    await openMenuAndPick(categoryButton(container, 'procedure'), 'refresh');
    await waitFor(() => {
      expect(mockGetDatabaseObjects).toHaveBeenCalledWith('conn-sql', 'procedure');
    });
    expect(container.querySelector('[data-item-name="pr_y"]')).not.toBeNull();

    // Connection context menu → refresh → loadForConnection for single-db.
    mockGetDatabases.mockClear();
    mockGetTables.mockClear();
    await triggerConnectionRefresh(findByText, 'SQLite Conn');
    await waitFor(() => {
      expect(mockGetDatabases).toHaveBeenCalledWith('conn-sql');
      expect(mockGetTables).toHaveBeenCalledWith('conn-sql', '/data/app.db');
    });

    // Database context menu → refresh → non-multi-db reloads via loadForConnection.
    mockGetDatabases.mockClear();
    await triggerContextMenuRefresh((await findByText('/data/app.db')).closest('button')!);
    await waitFor(() => {
      expect(mockGetDatabases).toHaveBeenCalled();
    });
  });

  it('F1-BUG-005: connection refresh restores expanded object categories', async () => {
    const { container, findByText } = await renderWithSqlite(
      [{ name: 'settings', tableType: 'table', schema: null }],
      {},
      {},
    );
    await findByText('settings');

    // Expand the procedure category so a refresh has live category state.
    mockGetDatabaseObjects.mockResolvedValue([{ name: 'pr_x', kind: 'procedure' }]);
    fireEvent.click(categoryButton(container, 'procedure'));
    await waitFor(() => {
      expect(container.querySelector('[data-item-name="pr_x"]')).not.toBeNull();
    });
    expect(categoryButton(container, 'procedure').textContent).toContain('1');

    // Connection-level refresh bumps schemaEpoch → epoch-triggered cache
    // invalidation must not leave the expanded category empty. The recovery
    // wave re-fetches it and the row keeps its entries + count.
    mockGetDatabaseObjects.mockClear();
    await triggerConnectionRefresh(findByText, 'SQLite Conn');

    await waitFor(() => {
      expect(mockGetDatabaseObjects).toHaveBeenCalledWith('conn-sql', 'procedure');
    });
    await waitFor(() => {
      expect(container.querySelector('[data-item-name="pr_x"]')).not.toBeNull();
    });
    expect(categoryButton(container, 'procedure').textContent).toContain('1');
  });

  it('F1-BUG-005: single-db database-node refresh restores expanded categories', async () => {
    const { container, findByText } = await renderWithSqlite(
      [{ name: 'settings', tableType: 'table', schema: null }],
      {},
      {},
    );
    await findByText('settings');

    mockGetDatabaseObjects.mockResolvedValue([{ name: 'fn_y', kind: 'function' }]);
    fireEvent.click(categoryButton(container, 'function'));
    await waitFor(() => {
      expect(container.querySelector('[data-item-name="fn_y"]')).not.toBeNull();
    });

    // Database-node refresh on a single-db tree goes through loadForConnection
    // too — the expanded function category must recover there as well.
    mockGetDatabaseObjects.mockClear();
    await triggerDatabaseRefresh(findByText, '/data/app.db');

    await waitFor(() => {
      expect(mockGetDatabaseObjects).toHaveBeenCalledWith('conn-sql', 'function');
    });
    await waitFor(() => {
      expect(container.querySelector('[data-item-name="fn_y"]')).not.toBeNull();
    });
    expect(categoryButton(container, 'function').textContent).toContain('1');
  });

  it('schema-level refresh reloads expanded schema-scoped categories', async () => {
    const conn = makeConn({
      id: 'cfg-sql',
      name: 'SQLite Conn',
      databaseType: 'sqlite',
      database: '/data/app.db',
    });
    connectionsState.connections = [conn];
    activeConnectionsState.connections = {
      'cfg-sql': { status: 'connected', dbSessionId: 'conn-sql', connectionId: 'cfg-sql' },
    };
    mockGetDatabases.mockResolvedValue(['/data/app.db']);
    mockGetTables.mockResolvedValue([
      { name: 'settings', tableType: 'table', schema: 'main' },
    ] as TableInfo[]);
    const onShowMessage = vi.fn();
    const { container, findByText } = render(
      <ConnectionNavigatorTree
        {...baseProps}
        activeConnectionId="cfg-sql"
        onShowMessage={onShowMessage}
      />,
    );
    await findByText('/data/app.db');
    await settleSessionLoad('conn-sql');

    // Schema grouping only kicks in for truthy schema names.
    await waitFor(() => {
      expect(container.querySelector('[data-schema-name="main"]')).not.toBeNull();
    });
    fireEvent.click(container.querySelector('[data-schema-name="main"]')!.closest('button')!);
    await waitFor(() => {
      expect(categoryButton(container, 'function')).toBeTruthy();
    });
    mockGetDatabaseObjects.mockResolvedValue([{ name: 'fn_z', kind: 'function' }]);
    fireEvent.click(categoryButton(container, 'function'));
    await waitFor(() => {
      expect(container.querySelector('[data-item-name="fn_z"]')).not.toBeNull();
    });
    mockGetTables.mockClear();

    await triggerSchemaRefresh(findByText, 'main');

    // Schema refresh reloads the tables of the schema's parent database.
    await waitFor(() => {
      expect(mockGetTables).toHaveBeenCalledWith('conn-sql', '/data/app.db');
    });

    // The expanded schema-scoped category reloads through its own menu.
    mockGetDatabaseObjects.mockClear();
    await openMenuAndPick(categoryButton(container, 'function'), 'refresh');
    await waitFor(() => {
      expect(mockGetDatabaseObjects).toHaveBeenCalledWith('conn-sql', 'function');
    });
    await waitFor(() => {
      expect(container.querySelector('[data-item-name="fn_z"]')).not.toBeNull();
    });
  });

  it('renders only the connection row when no database can be resolved', async () => {
    connectionsState.connections = [
      makeConn({ id: 'cfg-sql', name: 'Bare SQLite', databaseType: 'sqlite' }),
    ];
    activeConnectionsState.connections = {
      'cfg-sql': { status: 'connected', dbSessionId: 'conn-sql', connectionId: 'cfg-sql' },
    };
    mockGetDatabases.mockResolvedValue([]);
    const { container, findByText } = render(
      <ConnectionNavigatorTree {...baseProps} activeConnectionId="cfg-sql" />,
    );
    await findByText('Bare SQLite');
    await settleSessionLoad('conn-sql');
    await waitFor(() => {
      expect(useSchemaStore.getState().schemas.get('conn-sql')?.loading).toBe(false);
    });
    expect(container.querySelector('[data-tree-node="db"]')).toBeNull();
  });

  it('shows a loading row while single-db tables are being fetched', async () => {
    connectionsState.connections = [
      makeConn({
        id: 'cfg-sql',
        name: 'Slow SQLite',
        databaseType: 'sqlite',
        database: '/data/app.db',
      }),
    ];
    activeConnectionsState.connections = {
      'cfg-sql': { status: 'connected', dbSessionId: 'conn-sql', connectionId: 'cfg-sql' },
    };
    mockGetDatabases.mockResolvedValue(['/data/app.db']);
    mockGetTables.mockReturnValue(new Promise<TableInfo[]>(() => {}));
    const { findByText } = render(
      <ConnectionNavigatorTree {...baseProps} activeConnectionId="cfg-sql" />,
    );
    await findByText('/data/app.db');
    await findByText('common.loading');
  });
});

// Helper placed after the suite above for readability; hoisted function decl.
async function renderWithSqlite(
  tableItems: TableInfo[],
  connOverrides: Partial<ConnectionConfig>,
  props: {
    onSelectTable?: (name: string, schema?: string, db?: string) => void;
    onNodeContextMenu?: (payload: { kind: string; name: string }) => void;
    viewActions?: Record<string, (...args: unknown[]) => void>;
  },
) {
  connectionsState.connections = [
    makeConn({
      id: 'cfg-sql',
      name: 'SQLite Conn',
      databaseType: 'sqlite',
      database: '/data/app.db',
      ...connOverrides,
    }),
  ];
  activeConnectionsState.connections = {
    'cfg-sql': { status: 'connected', dbSessionId: 'conn-sql', connectionId: 'cfg-sql' },
  };
  mockGetDatabases.mockResolvedValue(['/data/app.db']);
  mockGetTables.mockImplementation((_c: string, db: string) =>
    db === '/data/app.db' ? Promise.resolve(tableItems) : Promise.resolve([]),
  );
  return render(
    <ConnectionNavigatorTree
      {...baseProps}
      activeConnectionId="cfg-sql"
      onSelectTable={props.onSelectTable ?? baseProps.onSelectTable}
      onNodeContextMenu={
        props.onNodeContextMenu as ConnectionNavigatorTreeProps['onNodeContextMenu']
      }
      viewActions={props.viewActions as ConnectionNavigatorTreeProps['viewActions']}
    />,
  );
}

/** Render a connected PostgreSQL session with three well-known schemas. */
async function renderPgTree(extraProps: Partial<ConnectionNavigatorTreeProps> = {}) {
  connectionsState.connections = [
    makeConn({ id: 'cfg-pg', name: 'PG Conn', databaseType: 'postgresql', port: 5432 }),
  ];
  activeConnectionsState.connections = {
    'cfg-pg': { status: 'connected', dbSessionId: 'conn-pg', connectionId: 'cfg-pg' },
  };
  mockGetDatabases.mockResolvedValue(['db_a']);
  mockGetTables.mockImplementation((_c: string, db: string) =>
    db === 'db_a'
      ? Promise.resolve([
          { name: 'users', tableType: 'table', schema: 'public' },
          { name: 'info_t', tableType: 'table', schema: 'information_schema' },
          { name: 'cat_t', tableType: 'table', schema: 'pg_catalog' },
        ] as TableInfo[])
      : Promise.resolve([]),
  );
  return render(
    <ConnectionNavigatorTree {...baseProps} activeConnectionId="cfg-pg" {...extraProps} />,
  );
}

describe('ConnectionNavigatorTree multi-db tree variants', () => {
  it('sorts schemas with the driver default schema first', async () => {
    const { container } = await renderPgTree();
    mockGetTables.mockImplementation((_c: string, db: string) =>
      db === 'db_a'
        ? Promise.resolve([
            { name: 't_zeta', tableType: 'table', schema: 'zeta' },
            { name: 't_pub', tableType: 'table', schema: 'public' },
            { name: 't_alpha', tableType: 'table', schema: 'alpha' },
          ] as TableInfo[])
        : Promise.resolve([]),
    );

    await waitFor(() => {
      const names = [...container.querySelectorAll('[data-schema-name]')].map((el) =>
        el.getAttribute('data-schema-name'),
      );
      expect(names).toEqual(['public', 'alpha', 'zeta']);
    });
  });

  it('skips schema grouping when the sole schema equals the database name', async () => {
    const { container, findByText } = render(<ConnectionNavigatorTree {...baseProps} />);
    await findByText('db_a');
    await settleSessionLoad('conn-1');
    mockGetTables.mockImplementation((_c: string, db: string) =>
      db === 'db_a'
        ? Promise.resolve([
            { name: 't1', tableType: 'table', schema: 'db_a' },
            { name: 't2', tableType: 'table', schema: 'db_a' },
          ] as TableInfo[])
        : Promise.resolve([]),
    );
    // Re-expand to pick up the new table payload.
    fireEvent.click((await findByText('db_a')).closest('button')!);
    await waitFor(() => {
      expect(mockGetTables).toHaveBeenCalledWith('conn-1', 'db_a');
    });
    fireEvent.click((await findByText('db_a')).closest('button')!);
    await waitFor(() => {
      expect(container.querySelector('[data-cat-id="tables"]')).not.toBeNull();
      expect(container.querySelector('[data-tree-node="schema"]')).toBeNull();
    });
  });

  it('hides system databases and system tables via the object filter', async () => {
    connectionsState.connections = [
      makeConn({
        id: 'cfg-filtered',
        name: 'Filtered MySQL',
        options: { objectFilter: { hideSystemSchemas: true } },
      }),
    ];
    activeConnectionsState.connections = {
      'cfg-filtered': {
        status: 'connected',
        dbSessionId: 'conn-f',
        connectionId: 'cfg-filtered',
      },
    };
    mockGetDatabases.mockResolvedValue(['db_visible', 'postgres', 'sys']);
    mockGetTables.mockImplementation((_c: string, db: string) =>
      db === 'db_visible'
        ? Promise.resolve([
            { name: 'real_table', tableType: 'table', schema: null },
            { name: 'internal', tableType: 'systemTable', schema: null },
          ] as TableInfo[])
        : Promise.resolve([]),
    );

    const { container, findByText } = render(
      <ConnectionNavigatorTree {...baseProps} activeConnectionId="cfg-filtered" />,
    );
    await findByText('db_visible');
    await settleSessionLoad('conn-f');

    await waitFor(() => {
      expect(container.querySelector('[data-db-name="db_visible"]')).not.toBeNull();
    });
    expect(container.querySelector('[data-db-name="postgres"]')).toBeNull();
    expect(container.querySelector('[data-db-name="sys"]')).toBeNull();
  });

  it('category context-menu refresh reloads tables for a multi-db driver', async () => {
    const { findByText, queryAllByText } = render(<ConnectionNavigatorTree {...baseProps} />);
    await ensureDbTableVisible(findByText, queryAllByText, 'db_a', 'users');
    mockGetTables.mockClear();

    // The category lives under the db row; right-click the category directly.
    const catButton = (await findByText('schemaTree.tables')).closest('button')!;
    fireEvent.contextMenu(catButton);
    await waitFor(() => {
      const items = vi.mocked(showWebContextMenu).mock.calls.at(-1)?.[0] ?? [];
      expect(items.some((item) => item.id === 'refresh')).toBe(true);
    });
    const items = vi.mocked(showWebContextMenu).mock.calls.at(-1)?.[0] ?? [];
    items.find((item) => item.id === 'refresh')?.action?.();

    await waitFor(() => {
      expect(mockGetTables).toHaveBeenCalledWith('conn-1', 'db_a');
    });
  });

  it('uses cached fallback tables when dropping the active database', async () => {
    const onShowMessage = vi.fn();
    const { findByText, queryAllByText } = render(
      <ConnectionNavigatorTree {...baseProps} onShowMessage={onShowMessage} />,
    );

    // Cache tables for db_a and for the fallback target postgres.
    await ensureDbTableVisible(findByText, queryAllByText, 'db_a', 'users');
    mockGetTables.mockImplementation((_c: string, db: string) =>
      db === 'db_a'
        ? Promise.resolve([{ name: 'users', tableType: 'table', schema: null }])
        : db === 'postgres'
          ? Promise.resolve([{ name: 'pgtbl', tableType: 'table', schema: null }])
          : Promise.resolve([]),
    );
    fireEvent.click((await findByText('postgres')).closest('button')!);
    await waitFor(() => {
      expect(mockGetTables).toHaveBeenCalledWith('conn-1', 'postgres');
    });
    // Make db_a the active database again by opening one of its tables —
    // only an *active* drop triggers the cached-fallback switch.
    fireEvent.click((await findByText('users')).closest('button')!);
    await waitFor(() => {
      expect(useSchemaStore.getState().schemas.get('conn-1')?.currentDatabase).toBe('db_a');
    });

    // The post-drop reload observes a database list where db_a is gone.
    mockGetDatabases.mockResolvedValue(['postgres', 'db_b']);

    await triggerDropDatabase(findByText, 'db_a');

    await waitFor(() => {
      expect(mockGetTables).toHaveBeenCalledWith('conn-1', 'postgres');
      expect(mockDriverExecute).toHaveBeenCalledWith({
        dbSessionId: 'conn-1',
        command: 'drop_database',
        input: { name: 'db_a' },
      });
    });
    await waitFor(() => {
      expect(useSchemaStore.getState().schemas.get('conn-1')?.currentDatabase).toBe('postgres');
    });
    expect(useSchemaStore.getState().currentDatabase).toBe('postgres');
    expect(onShowMessage).not.toHaveBeenCalled();
  });

  it('aborts the drop when confirmation is declined', async () => {
    const onShowMessage = vi.fn();
    confirmMock.mockResolvedValueOnce(false);
    const { findByText } = render(
      <ConnectionNavigatorTree {...baseProps} onShowMessage={onShowMessage} />,
    );
    await findByText('db_a');
    await triggerDropDatabase(findByText, 'db_a');
    expect(mockDriverExecute).not.toHaveBeenCalled();
    expect(onShowMessage).not.toHaveBeenCalled();
  });

  it('falls back to another listed database when postgres is absent', async () => {
    connectionsState.connections = [makeConn({ id: 'cfg-mysql', name: 'Local MySQL' })];
    mockGetDatabases.mockResolvedValue(['first', 'second']);
    const { findByText } = render(<ConnectionNavigatorTree {...baseProps} />);

    await findByText('first');
    await settleSessionLoad('conn-1');

    // The post-drop reload observes the list without `first`, so the fallback
    // database stays the active one instead of snapping back to the dropped db.
    mockGetDatabases.mockResolvedValue(['second']);

    await triggerDropDatabase(findByText, 'first');

    await waitFor(() => {
      expect(mockDriverExecute).toHaveBeenCalledWith({
        dbSessionId: 'conn-1',
        command: 'drop_database',
        input: { name: 'first' },
      });
      expect(useSchemaStore.getState().currentDatabase).toBe('second');
    });
  });

  it('keeps the local state untouched when the dropped db is the last one', async () => {
    mockGetDatabases.mockResolvedValue(['only_db']);
    const onShowMessage = vi.fn();
    const { findByText } = render(
      <ConnectionNavigatorTree {...baseProps} onShowMessage={onShowMessage} />,
    );

    await findByText('only_db');
    await settleSessionLoad('conn-1');
    await triggerDropDatabase(findByText, 'only_db');

    await waitFor(() => {
      expect(mockDriverExecute).toHaveBeenCalledWith({
        dbSessionId: 'conn-1',
        command: 'drop_database',
        input: { name: 'only_db' },
      });
    });
    expect(onShowMessage).not.toHaveBeenCalled();
  });

  it('dispatches auxiliary database context-menu actions', async () => {
    const openSqlFile = vi.fn();
    const createTable = vi.fn();
    const openQueryHistory = vi.fn();
    const openErDiagram = vi.fn();
    const { findByText, queryAllByText } = render(
      <ConnectionNavigatorTree
        {...baseProps}
        viewActions={{ openSqlFile, createTable, openQueryHistory, openErDiagram }}
      />,
    );
    await ensureDbTableVisible(findByText, queryAllByText, 'db_a', 'users');
    const dbButton = (await findByText('db_a')).closest('button')!;

    await openMenuAndPick(dbButton, 'copy-database-name');
    expect(mockWriteText).toHaveBeenCalledWith('db_a');

    await openMenuAndPick(dbButton, 'view-er-diagram');
    expect(openErDiagram).toHaveBeenCalled();

    await openMenuAndPick(dbButton, 'query-history');
    expect(openQueryHistory).toHaveBeenCalled();

    await openMenuAndPick(dbButton, 'execute-sql-file');
    expect(openSqlFile).toHaveBeenCalled();

    await openMenuAndPick(dbButton, 'new-table');
    expect(createTable).toHaveBeenCalled();

    await openMenuAndPick(dbButton, 'backup');
    expect(openBackupWindowMock).toHaveBeenCalledWith('backup', {
      connectionId: 'cfg-mysql',
      database: 'db_a',
    });

    await openMenuAndPick(dbButton, 'restore');
    expect(openBackupWindowMock).toHaveBeenCalledWith('restore', {
      connectionId: 'cfg-mysql',
      database: 'db_a',
    });
  });

  it('dispatches create-schema from the database menu for drivers supporting it', async () => {
    const openCreateSchema = vi.fn();
    const { findByText } = await renderPgTree({ viewActions: { openCreateSchema } });
    await findByText('db_a');
    const dbButton = (await findByText('db_a')).closest('button')!;
    await openMenuAndPick(dbButton, 'create-schema');
    expect(openCreateSchema).toHaveBeenCalled();
    expect(useSchemaStore.getState().currentDatabase).toBe('db_a');
  });
});

describe('ConnectionNavigatorTree schema context menu', () => {
  it('copies schema name and dispatches sql-file/new-table/history/transfer actions', async () => {
    const openSqlFile = vi.fn();
    const createTable = vi.fn();
    const openQueryHistory = vi.fn();
    const openErDiagram = vi.fn();
    const { findByText } = await renderPgTree({
      viewActions: { openSqlFile, createTable, openQueryHistory, openErDiagram },
    });
    await findByText('db_a');
    const schemaButton = (await findByText('public')).closest('button')!;

    await openMenuAndPick(schemaButton, 'copy-schema-name');
    expect(mockWriteText).toHaveBeenCalledWith('public');

    await openMenuAndPick(schemaButton, 'view-er-diagram');
    expect(openErDiagram).toHaveBeenCalled();

    await openMenuAndPick(schemaButton, 'query-history');
    expect(openQueryHistory).toHaveBeenCalled();

    await openMenuAndPick(schemaButton, 'execute-sql-file');
    expect(openSqlFile).toHaveBeenCalled();

    await openMenuAndPick(schemaButton, 'new-table');
    expect(createTable).toHaveBeenCalled();
  });

  it('drops a schema after confirmation and reloads the connection', async () => {
    const { findByText } = await renderPgTree();
    await findByText('db_a');
    mockGetDatabases.mockClear();

    await openMenuAndPick((await findByText('public')).closest('button')!, 'drop-schema');

    await waitFor(() => {
      expect(mockDriverExecute).toHaveBeenCalledWith({
        dbSessionId: 'conn-pg',
        command: 'drop_schema',
        input: { name: 'public', cascade: true },
        database: 'db_a',
      });
      expect(mockGetTables).toHaveBeenCalledWith('conn-pg', 'db_a');
    });
  });

  it('reports an error when dropping a schema fails', async () => {
    const onShowMessage = vi.fn();
    const { findByText } = await renderPgTree({ onShowMessage });
    await findByText('db_a');
    mockDriverExecute.mockRejectedValueOnce(new Error('no rights'));

    await openMenuAndPick((await findByText('public')).closest('button')!, 'drop-schema');

    await waitFor(() => {
      expect(onShowMessage).toHaveBeenCalledWith('no rights', 'error');
    });
  });

  it('hides drop-schema for protected system schemas', async () => {
    const { findByText } = await renderPgTree();
    await findByText('db_a');

    for (const schema of ['information_schema', 'pg_catalog']) {
      fireEvent.contextMenu((await findByText(schema)).closest('button')!);
      await waitFor(() => expect(lastMenuItems().length).toBeGreaterThan(0));
      expect(lastMenuItems().some((i) => i.id === 'drop-schema')).toBe(false);
    }
  });

  it('drops a table after confirmation and reloads tables', async () => {
    const { findByText, queryAllByText } = await renderPgTree();
    await waitFor(() => findByText('db_a'));
    fireEvent.click((await findByText('public')).closest('button')!);
    const tablesCategory = await waitFor(() => {
      const nodes = queryAllByText('schemaTree.tables');
      expect(nodes.length).toBeGreaterThan(0);
      return nodes[nodes.length - 1]!;
    });
    fireEvent.click(tablesCategory.closest('button')!);
    mockGetTables.mockClear();

    await openMenuAndPick((await findByText('users')).closest('button')!, 'drop');

    await waitFor(() => {
      expect(mockExecuteQuery).toHaveBeenCalledWith(
        'conn-pg',
        'DROP TABLE "public"."users"',
        undefined,
        'db_a',
        'public',
      );
      expect(mockGetTables).toHaveBeenCalledWith('conn-pg', 'db_a');
    });
  });

  it('pins drop table to the right-clicked database for MySQL', async () => {
    const { findByText, queryAllByText } = render(<ConnectionNavigatorTree {...baseProps} />);
    await ensureDbTableVisible(findByText, queryAllByText, 'db_a', 'users');
    await ensureDbTableVisible(findByText, queryAllByText, 'db_b', 'orders');
    await waitFor(() => {
      expect(useSchemaStore.getState().currentDatabase).toBe('db_b');
    });
    await activateDatabaseContext(findByText, 'db_a', 'users');
    mockExecuteQuery.mockClear();

    await openMenuAndPick((await findByText('orders')).closest('button')!, 'drop');

    await waitFor(() => {
      expect(mockExecuteQuery).toHaveBeenCalledWith(
        'conn-1',
        'DROP TABLE `orders`',
        undefined,
        'db_b',
        null,
      );
    });
  });

  it('pins drop_schema to the right-clicked database while session context stays on another db', async () => {
    connectionsState.connections = [
      makeConn({ id: 'cfg-pg', name: 'PG Conn', databaseType: 'postgresql', port: 5432 }),
    ];
    activeConnectionsState.connections = {
      'cfg-pg': { status: 'connected', dbSessionId: 'conn-pg', connectionId: 'cfg-pg' },
    };
    mockGetDatabases.mockResolvedValue(['db_a', 'db_b']);
    mockGetTables.mockImplementation((_c: string, db: string) =>
      db === 'db_a'
        ? Promise.resolve([{ name: 'users', tableType: 'table', schema: 'public' }] as TableInfo[])
        : db === 'db_b'
          ? Promise.resolve([
              { name: 't1', tableType: 'table', schema: 'analytics' },
            ] as TableInfo[])
          : Promise.resolve([]),
    );

    const { findByText } = render(
      <ConnectionNavigatorTree {...baseProps} activeConnectionId="cfg-pg" />,
    );
    await waitFor(() => findByText('db_b'));
    seedSessionSchema('conn-pg', {
      currentDatabase: 'db_a',
      databases: ['db_a', 'db_b'],
    });
    useSchemaStore.setState({ currentDatabase: 'db_a' });

    fireEvent.click((await findByText('db_b')).closest('button')!);
    await waitFor(() => {
      expect(mockGetTables).toHaveBeenCalledWith('conn-pg', 'db_b');
    });
    await findByText('analytics');
    mockDriverExecute.mockClear();

    await openMenuAndPick((await findByText('analytics')).closest('button')!, 'drop-schema');

    await waitFor(() => {
      expect(mockDriverExecute).toHaveBeenCalledWith({
        dbSessionId: 'conn-pg',
        command: 'drop_schema',
        input: { name: 'analytics', cascade: true },
        database: 'db_b',
      });
    });
  });

  it('pins truncate table to the right-clicked database while session context stays on another db', async () => {
    const { findByText, queryAllByText } = render(<ConnectionNavigatorTree {...baseProps} />);
    await ensureDbTableVisible(findByText, queryAllByText, 'db_a', 'users');
    await ensureDbTableVisible(findByText, queryAllByText, 'db_b', 'orders');
    await activateDatabaseContext(findByText, 'db_a', 'users');
    mockExecuteQuery.mockClear();

    await openMenuAndPick((await findByText('orders')).closest('button')!, 'truncate');

    await waitFor(() => {
      expect(mockExecuteQuery).toHaveBeenCalledWith(
        'conn-1',
        'TRUNCATE TABLE `orders`',
        undefined,
        'db_b',
        null,
      );
    });
  });

  it('reports an error when dropping a table fails', async () => {
    const onShowMessage = vi.fn();
    const { findByText, queryAllByText } = await renderPgTree({ onShowMessage });
    await waitFor(() => findByText('db_a'));
    fireEvent.click((await findByText('public')).closest('button')!);
    const tablesCategory = await waitFor(() => {
      const nodes = queryAllByText('schemaTree.tables');
      expect(nodes.length).toBeGreaterThan(0);
      return nodes[nodes.length - 1]!;
    });
    fireEvent.click(tablesCategory.closest('button')!);
    mockExecuteQuery.mockRejectedValueOnce(new Error('permission denied'));

    await openMenuAndPick((await findByText('users')).closest('button')!, 'drop');

    await waitFor(() => {
      expect(onShowMessage).toHaveBeenCalledWith('permission denied', 'error');
    });
  });
});

describe('ConnectionNavigatorTree group management', () => {
  it('collapses and re-expands a group from its header', async () => {
    const { container, findByText, queryByText } = render(
      <ConnectionNavigatorTree {...baseProps} />,
    );
    await findByText('db_a');
    const header = container.querySelector('[data-group-header]')!;
    fireEvent.click(header);
    await waitFor(() => expect(queryByText('db_a')).toBeNull());
    fireEvent.click(header);
    await waitFor(() => expect(queryByText('db_a')).not.toBeNull());
  });

  it('creates, renames and deletes groups from the group context menu', async () => {
    connectionsState.groups = ['work'];
    connectionsState.connections = [makeConn({ id: 'cfg-w', name: 'Work Conn', group: 'work' })];
    const { container, findByText } = render(<ConnectionNavigatorTree {...baseProps} />);
    await findByText('Work Conn');
    const header = [...container.querySelectorAll('[data-group-header]')].find((el) =>
      el.textContent?.includes('work'),
    )!;
    expect(header).toBeTruthy();

    // Rename prefills the current label and commits on Enter.
    await openMenuAndPick(header, 'rename-group');
    const renameDialog = document.querySelector(
      '[role="dialog"][aria-label="main.ctx.renameGroup"]',
    )!;
    const renameInput = renameDialog.querySelector('input')!;
    expect((renameInput as HTMLInputElement).value).toBe('work');
    fireEvent.change(renameInput, { target: { value: 'work2' } });
    fireEvent.keyDown(renameInput, { key: 'Enter' });
    await waitFor(() => {
      expect(connectionsState.renameGroup).toHaveBeenCalledWith('work', 'work2');
    });

    // Delete asks for confirmation then dispatches.
    await openMenuAndPick(header, 'delete-group');
    await waitFor(() => {
      expect(connectionsState.deleteGroup).toHaveBeenCalledWith('work');
    });

    // New group dialog commits on Enter.
    await openMenuAndPick(header, 'new-group');
    const dialog = document.querySelector('[role="dialog"][aria-label="common.newGroup"]')!;
    const input = dialog.querySelector('input')!;
    fireEvent.change(input, { target: { value: 'fresh' } });
    fireEvent.keyDown(input, { key: 'Enter' });
    await waitFor(() => {
      expect(connectionsState.addGroup).toHaveBeenCalledWith('fresh');
    });
  });

  it('auto-expands groups added after mount', async () => {
    const view = render(<ConnectionNavigatorTree {...baseProps} />);
    await view.findByText('Local MySQL');

    connectionsState.groups = ['', 'late'];
    view.rerender(<ConnectionNavigatorTree {...baseProps} />);

    await view.findByText('late');
    // The new group is expanded immediately and renders its empty hint.
    expect(view.getByText('main.noConnections')).toBeTruthy();
  });
});

describe('ConnectionNavigatorTree group dialogs', () => {
  it('opens the new-group dialog from the toolbar and validates input', async () => {
    const { container, findByText } = render(<ConnectionNavigatorTree {...baseProps} />);
    await findByText('Local MySQL');
    const toolbarBtn = container.querySelector('button[title="common.newGroup"]')!;

    // Empty name → OK just closes without dispatching.
    fireEvent.click(toolbarBtn);
    const dialog = document.querySelector('[role="dialog"][aria-label="common.newGroup"]')!;
    fireEvent.click(
      [...dialog.querySelectorAll('button')].find((b) => b.textContent === 'common.ok')!,
    );
    expect(connectionsState.addGroup).not.toHaveBeenCalled();
    expect(document.querySelector('[role="dialog"]')).toBeNull();

    // Valid name via OK button.
    fireEvent.click(toolbarBtn);
    const dialog2 = document.querySelector('[role="dialog"][aria-label="common.newGroup"]')!;
    const input = dialog2.querySelector('input')!;
    fireEvent.change(input, { target: { value: 'grp' } });
    fireEvent.click(
      [...dialog2.querySelectorAll('button')].find((b) => b.textContent === 'common.ok')!,
    );
    await waitFor(() => {
      expect(connectionsState.addGroup).toHaveBeenCalledWith('grp');
    });

    // Cancel closes without dispatching.
    fireEvent.click(toolbarBtn);
    const dialog3 = document.querySelector('[role="dialog"][aria-label="common.newGroup"]')!;
    fireEvent.click(
      [...dialog3.querySelectorAll('button')].find((b) => b.textContent === 'common.cancel')!,
    );
    expect(document.querySelector('[role="dialog"]')).toBeNull();
  });

  it('rename-group dialog cancel leaves the group untouched', async () => {
    connectionsState.groups = ['work'];
    connectionsState.connections = [makeConn({ id: 'cfg-w', name: 'Work Conn', group: 'work' })];
    const { container, findByText } = render(<ConnectionNavigatorTree {...baseProps} />);
    await findByText('Work Conn');
    const header = [...container.querySelectorAll('[data-group-header]')].find((el) =>
      el.textContent?.includes('work'),
    )!;
    await openMenuAndPick(header, 'rename-group');
    const dialog = document.querySelector('[role="dialog"][aria-label="main.ctx.renameGroup"]')!;
    fireEvent.click(
      [...dialog.querySelectorAll('button')].find((b) => b.textContent === 'common.cancel')!,
    );
    expect(connectionsState.renameGroup).not.toHaveBeenCalled();
    expect(document.querySelector('[role="dialog"]')).toBeNull();
  });
});

describe('ConnectionNavigatorTree connection context menu actions', () => {
  async function setupSweep(viewActions: ConnectionNavigatorTreeProps['viewActions']) {
    connectionsState.groups = ['work'];
    mockGetDriverCommands.mockResolvedValue([
      { id: 'server_status_snapshot' },
      { id: 'list_processes' },
    ]);
    const view = render(<ConnectionNavigatorTree {...baseProps} viewActions={viewActions} />);
    await view.findByText('db_a');
    return connRow(view.container, 'Local MySQL');
  }

  it('dispatches every connection-level action with the right target', async () => {
    const viewActions = {
      newQuery: vi.fn(),
      openQueryHistory: vi.fn(),
      openCreateDatabase: vi.fn(),
      openCreateUser: vi.fn(),
      openServerStatus: vi.fn(),
      openProcessList: vi.fn(),
    };
    const row = await setupSweep(viewActions);

    await openMenuAndPick(row, 'disconnect');
    expect(baseProps.onDisconnect).toHaveBeenCalledWith('cfg-mysql');

    await openMenuAndPick(row, 'copy-name');
    expect(mockWriteText).toHaveBeenCalledWith('Local MySQL');

    await openMenuAndPick(row, 'copy-connection-url');
    expect(mockWriteText).toHaveBeenCalledWith(expect.stringContaining('mysql://'));

    await openMenuAndPick(row, 'new-query');
    expect(baseProps.onSelectConnection).toHaveBeenCalledWith('cfg-mysql');
    expect(viewActions.newQuery).toHaveBeenCalled();

    await openMenuAndPick(row, 'query-history');
    expect(viewActions.openQueryHistory).toHaveBeenCalled();

    await openMenuAndPick(row, 'create-database');
    expect(viewActions.openCreateDatabase).toHaveBeenCalled();

    await openMenuAndPick(row, 'create-user');
    expect(viewActions.openCreateUser).toHaveBeenCalled();

    await openMenuAndPick(row, 'process-list');
    expect(viewActions.openProcessList).toHaveBeenCalledWith(
      expect.objectContaining({ connectionId: 'cfg-mysql', dbSessionId: 'conn-1' }),
    );

    await openMenuAndPick(row, 'server-status');
    expect(viewActions.openServerStatus).toHaveBeenCalledWith(
      expect.objectContaining({ connectionId: 'cfg-mysql', dbSessionId: 'conn-1' }),
    );

    await openMenuAndPick(row, 'pin-connection');
    expect(connectionsState.toggleConnectionPinned).toHaveBeenCalledWith('cfg-mysql');

    await openMenuAndPick(row, 'backup');
    expect(openBackupWindowMock).toHaveBeenCalledWith('backup', {
      connectionId: 'cfg-mysql',
      database: undefined,
    });

    await openMenuAndPick(row, 'restore');
    expect(openBackupWindowMock).toHaveBeenCalledWith('restore', {
      connectionId: 'cfg-mysql',
      database: undefined,
    });

    await openMenuAndPick(row, 'edit-connection');
    expect(baseProps.onEditConnection).toHaveBeenCalledWith('cfg-mysql');

    await openMenuAndPick(row, 'duplicate-connection');
    expect(connectionsState.duplicateConnection).toHaveBeenCalledWith('cfg-mysql');

    await openMenuAndPick(row, 'delete-connection');
    expect(baseProps.onDeleteConnection).toHaveBeenCalledWith('cfg-mysql');

    // Submenu: move to another group.
    fireEvent.contextMenu(row);
    await waitFor(() => {
      expect(lastMenuItems().some((i) => i.id === 'organize-submenu')).toBe(true);
    });
    const submenu = lastMenuItems().find((i) => i.id === 'organize-submenu')!;
    submenu.items!.find((i) => i.id === 'move-group-work')!.action!();
    expect(connectionsState.moveConnectionToGroup).toHaveBeenCalledWith('cfg-mysql', 'work');
  });

  it('moves a grouped connection out of its group via the submenu', async () => {
    connectionsState.groups = ['work'];
    connectionsState.connections = [makeConn({ id: 'cfg-w', name: 'Work Conn', group: 'work' })];
    const { container } = render(<ConnectionNavigatorTree {...baseProps} />);
    await waitFor(() => connRow(container, 'Work Conn'));
    const row = connRow(container, 'Work Conn');

    fireEvent.contextMenu(row);
    await waitFor(() => {
      expect(lastMenuItems().some((i) => i.id === 'organize-submenu')).toBe(true);
    });
    const submenu = lastMenuItems().find((i) => i.id === 'organize-submenu')!;
    submenu.items!.find((i) => i.id === 'remove-from-group')!.action!();
    expect(connectionsState.moveConnectionToGroup).toHaveBeenCalledWith('cfg-w', undefined);
  });

  it('opens an idle connection from the context menu', async () => {
    connectionsState.connections = [makeConn({ id: 'cfg-idle', name: 'Idle Conn' })];
    activeConnectionsState.connections = {};
    const { container } = render(<ConnectionNavigatorTree {...baseProps} />);
    await waitFor(() => connRow(container, 'Idle Conn'));

    await openMenuAndPick(connRow(container, 'Idle Conn'), 'open-connection');
    expect(baseProps.onSelectConnection).toHaveBeenCalledWith('cfg-idle');
    expect(mockConnect).toHaveBeenCalledWith(expect.objectContaining({ id: 'cfg-idle' }));
  });

  it('hides driver-command entries when discovery returns nothing', async () => {
    const { container } = render(<ConnectionNavigatorTree {...baseProps} />);
    await waitFor(() => connRow(container, 'Local MySQL'));
    fireEvent.contextMenu(connRow(container, 'Local MySQL'));
    await waitFor(() => expect(lastMenuItems().length).toBeGreaterThan(0));
    expect(lastMenuItems().some((i) => i.id === 'server-status')).toBe(false);
    expect(lastMenuItems().some((i) => i.id === 'process-list')).toBe(false);
  });

  it('object-filter action opens the dialog; saving persists prefs and refreshes', async () => {
    const { container, findByText } = render(<ConnectionNavigatorTree {...baseProps} />);
    await findByText('db_a');
    const row = connRow(container, 'Local MySQL');

    await openMenuAndPick(row, 'object-filter');
    const dialog = document.querySelector('[role="dialog"][aria-label="common.objectFilter"]')!;
    expect(dialog).toBeTruthy();

    mockGetDatabases.mockClear();
    fireEvent.click(
      [...dialog.querySelectorAll('button')].find((b) => b.textContent === 'common.save')!,
    );
    await waitFor(() => {
      expect(connectionsState.saveConnection).toHaveBeenCalledWith(
        expect.objectContaining({
          id: 'cfg-mysql',
          options: expect.objectContaining({ objectFilter: {} }),
        }),
      );
      expect(mockGetDatabases).toHaveBeenCalled(); // refreshConnection ran
    });
    await waitFor(() => {
      expect(document.querySelector('[role="dialog"]')).toBeNull();
    });
  });
});

describe('ConnectionNavigatorTree path-hierarchy namespace trees', () => {
  async function renderNamespaceTree(extraProps: Partial<ConnectionNavigatorTreeProps> = {}) {
    connectionsState.connections = [
      makeConn({ id: 'cfg-doris', name: 'Doris Conn', databaseType: 'doris' }),
    ];
    activeConnectionsState.connections = {
      'cfg-doris': { status: 'connected', dbSessionId: 'conn-doris', connectionId: 'cfg-doris' },
    };
    mockGetDatabases.mockResolvedValue(['db_x']);
    const view = render(
      <ConnectionNavigatorTree {...baseProps} activeConnectionId="cfg-doris" {...extraProps} />,
    );
    await settleSessionLoad('conn-doris');
    seedSessionSchema('conn-doris', {
      currentDatabase: 'db_x',
      namespaceTree: {
        // Table leaves are arrays; branches are plain objects.
        deep: { findme: [] },
        emptydir: {},
        mv1: [],
        public: { users: [], orders: [] },
      },
      tables: [
        { name: 'mv1', tableType: 'materializedView' },
        { name: 'users', tableType: 'table' },
      ] as TableInfo[],
      loadedPaths: new Set<string>(),
      pathItems: {},
    });
    await view.findByText('public');
    return view;
  }

  it('renders branches and typed leaves, and lazy-loads on expand', async () => {
    const onSelectTable = vi.fn();
    const { container, findByText } = await renderNamespaceTree({
      onSelectTable,
    });

    // Top-level leaves render immediately; materializedView maps to kind "view".
    const mvNode = container.querySelector('[data-item-name="mv1"]')!;
    expect(mvNode.getAttribute('data-tree-node')).toBe('view');

    // Branch starts collapsed.
    expect(container.querySelector('[data-item-name="users"]')).toBeNull();
    fireEvent.click((await findByText('public')).closest('button')!);

    await waitFor(() => {
      expect(container.querySelector('[data-item-name="users"]')).not.toBeNull();
    });
    // Expanding a branch triggers a namespace ensure for its path. The ensure
    // prefixes the current database root: fetch path is `db_x/public`.
    await waitFor(() => {
      expect(mockGetTables).toHaveBeenCalledWith('conn-doris', 'db_x/public');
    });

    // Leaf click selects; leaf context menu dispatches the mapped kind.
    // Re-query mv1: the virtual list recreates row nodes on every render.
    fireEvent.click(container.querySelector('[data-item-name="users"]')!);
    await waitFor(() => {
      expect(onSelectTable).toHaveBeenCalledWith('users', undefined, 'public');
    });

    const mvButton = container.querySelector('[data-item-name="mv1"]')!.closest('button')!;
    fireEvent.contextMenu(mvButton);
    await waitFor(() => {
      expect(lastMenuItems().some((i) => i.id === 'drop-view')).toBe(true);
    });

    // Collapse again hides the children.
    fireEvent.click((await findByText('public')).closest('button')!);
    await waitFor(() => {
      expect(container.querySelector('[data-item-name="users"]')).toBeNull();
    });

    // Unloaded empty branch renders a loading placeholder beneath it.
    fireEvent.click((await findByText('emptydir')).closest('button')!);
    await findByText('common.loading');
    await waitFor(() => {
      expect(mockGetTables).toHaveBeenCalledWith('conn-doris', 'db_x/emptydir');
    });
  });

  it('search prunes unmatched namespaces and force-expands matches', async () => {
    const { container } = await renderNamespaceTree();
    fireEvent.change(searchInput(container), { target: { value: 'findme' } });

    await waitFor(() => {
      expect(container.querySelector('[data-item-name="findme"]')).not.toBeNull();
    });
    expect(container.querySelector('[data-item-name="mv1"]')).toBeNull();
    expect(container.textContent).not.toContain('orders');
    expect(container.querySelector('[data-conn-name="Doris Conn"]')).not.toBeNull();
  });

  it('refreshing the connection re-runs the namespace ensure', async () => {
    const { findByText } = await renderNamespaceTree();
    mockGetDatabases.mockClear();
    await triggerConnectionRefresh(findByText, 'Doris Conn');
    await waitFor(() => {
      expect(mockGetDatabases).toHaveBeenCalledTimes(1);
    });
  });
});

describe('ConnectionNavigatorTree key-value stores', () => {
  function setupRedis() {
    connectionsState.connections = [
      makeConn({ id: 'cfg-kv', name: 'Redis Conn', databaseType: 'redis' }),
    ];
    activeConnectionsState.connections = {
      'cfg-kv': { status: 'connected', dbSessionId: 'conn-kv', connectionId: 'cfg-kv' },
    };
  }

  it('renders kv databases and routes clicks through onSelectKvDb', async () => {
    setupRedis();
    mockGetDatabases.mockResolvedValue(['db0', 'db1']);
    const onSelectKvDb = vi.fn();
    const { container, findByText } = render(
      <ConnectionNavigatorTree {...baseProps} onSelectKvDb={onSelectKvDb} />,
    );
    await findByText('db0');
    const kvNodes = container.querySelectorAll('[data-tree-node="kv-db"]');
    expect(kvNodes.length).toBe(2);

    fireEvent.click(kvNodes[1]!);
    expect(onSelectKvDb).toHaveBeenCalledWith('cfg-kv', 'db1');
  });

  it('falls back to connection+table selection when onSelectKvDb is missing', async () => {
    setupRedis();
    mockGetDatabases.mockResolvedValue(['db0']);
    const onSelectTable = vi.fn();
    const { container, findByText } = render(
      <ConnectionNavigatorTree {...baseProps} onSelectTable={onSelectTable} />,
    );
    await findByText('db0');
    fireEvent.click(container.querySelector('[data-tree-node="kv-db"]')!);
    expect(baseProps.onSelectConnection).toHaveBeenCalledWith('cfg-kv');
    expect(onSelectTable).toHaveBeenCalledWith('db0');
  });

  it('shows a loading placeholder while the database list is pending', async () => {
    setupRedis();
    mockGetDatabases.mockReturnValue(new Promise<string[]>(() => {}));
    const { findByText } = render(<ConnectionNavigatorTree {...baseProps} />);
    await findByText('common.loading');
  });
});

describe('ConnectionNavigatorTree imperative refresh guards', () => {
  it('ignores refresh requests for unknown or disconnected connections', async () => {
    const navigatorRef = createRef<ConnectionNavigatorTreeHandle>();
    const view = render(<ConnectionNavigatorTree {...baseProps} ref={navigatorRef} />);
    await view.findByText('db_a');
    mockGetDatabases.mockClear();

    await navigatorRef.current!.refreshConnection('unknown-id');
    expect(mockGetDatabases).not.toHaveBeenCalled();

    // Dropping the runtime session clears bookkeeping; refresh becomes a no-op.
    activeConnectionsState.connections = {};
    view.rerender(<ConnectionNavigatorTree {...baseProps} ref={navigatorRef} />);
    await navigatorRef.current!.refreshConnection('cfg-mysql');
    expect(mockGetDatabases).not.toHaveBeenCalled();
  });

  it('refreshAllConnections only touches connected sessions', async () => {
    connectionsState.connections = [MYSQL_CONN, makeConn({ id: 'cfg-idle', name: 'Idle Conn' })];
    activeConnectionsState.connections = {
      'cfg-mysql': { status: 'connected', dbSessionId: 'conn-1', connectionId: 'cfg-mysql' },
    };
    const navigatorRef = createRef<ConnectionNavigatorTreeHandle>();
    const view = render(<ConnectionNavigatorTree {...baseProps} ref={navigatorRef} />);
    await view.findByText('db_a');
    mockGetDatabases.mockClear();

    await navigatorRef.current!.refreshAllConnections();
    await waitFor(() => {
      expect(mockGetDatabases).toHaveBeenCalledTimes(1);
    });
  });

  it('query-history on a disconnected connection sets pendingQueryHistory instead of calling viewActions directly', async () => {
    connectionsState.connections = [makeConn({ id: 'cfg-idle', name: 'Idle Conn' })];
    activeConnectionsState.connections = {};
    panelStoreState.pendingQueryHistoryConnectionId = null;
    mockSetPendingQueryHistory.mockClear();

    const { container } = render(<ConnectionNavigatorTree {...baseProps} />);
    await waitFor(() => connRow(container, 'Idle Conn'));

    await openMenuAndPick(connRow(container, 'Idle Conn'), 'query-history');

    // Should open the connection
    expect(baseProps.onSelectConnection).toHaveBeenCalledWith('cfg-idle');
    // Should NOT call viewActions.openQueryHistory (actionsRef is null)
    // Instead should store pending intent in panelStore
    expect(mockSetPendingQueryHistory).toHaveBeenCalledWith('cfg-idle');
  });

  it('reloads expanded databases when the schema fingerprint changes', async () => {
    const view = render(<ConnectionNavigatorTree {...baseProps} />);
    await view.findByText('db_a');
    mockGetTables.mockClear();

    seedSessionSchema('conn-1', { schemaEpoch: 42 });

    await waitFor(() => {
      expect(mockGetTables).toHaveBeenCalledWith('conn-1', 'db_a');
    });
  });
});
