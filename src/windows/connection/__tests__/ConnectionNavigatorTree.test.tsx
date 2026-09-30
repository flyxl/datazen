import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, cleanup, waitFor, fireEvent } from '@testing-library/react';
import { createRef } from 'react';
import {
  ConnectionNavigatorTree,
  type ConnectionNavigatorTreeHandle,
  type ConnectionNavigatorTreeProps,
} from '../ConnectionNavigatorTree';
import { useSchemaStore } from '../../../stores/schemaStore';
import { createEmptyConnectionSchema } from '../../../stores/schemaStoreState';
import type { ConnectionConfig, TableInfo } from '../../../types';
import { showWebContextMenu } from '../../../stores/contextMenuStore';
import { getUnifiedRowKey } from '../navigator/utils';
import type { UnifiedRow } from '../navigator/types';
import type { NativeMenuItemDef } from '../../../lib/nativeContextMenu';
import type { SqlNamespace } from '../../../lib/sqlNamespace';

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
const mockGetOpenDatabases = vi.hoisted(() => vi.fn().mockResolvedValue([]));
const mockSaveConnection = vi.hoisted(() => vi.fn().mockResolvedValue(undefined));
const mockCloseDatabase = vi.hoisted(() => vi.fn().mockResolvedValue(true));
const openDataSyncWindowMock = vi.hoisted(() => vi.fn());
const openSchemaDiffWindowMock = vi.hoisted(() => vi.fn());
const openDataTransferWindowMock = vi.hoisted(() => vi.fn());

vi.mock('../../../commands/connection', () => ({
  connectionCommands: {
    reorderConnections: (...args: unknown[]) => mockReorderConnections(...args),
    saveConnection: (...args: unknown[]) => mockSaveConnection(...args),
    getOpenDatabases: (...args: unknown[]) => mockGetOpenDatabases(...args),
    closeDatabase: (...args: unknown[]) => mockCloseDatabase(...args),
  },
}));

vi.mock('../../../extensions/generated', () => {
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
    DRIVER_ICON_ENTRIES: {},
    DRIVER_ICON_PARENTS: {},
    DRIVER_SQL_DIALECTS: {},
    getDriverSchemaTree: () => undefined,
    getDriverConnectionForm: () => undefined,
    getDriverConnectionAdvanced: () => undefined,
    getDriverValidator: () => undefined,
    getDriverClipboardParsers: () => [],
  };
});

/**
 * Painting window for the virtualizer mock below. `size: Infinity` (the
 * default) paints every row, which is what most suites assume. Tests that
 * exercise the "only the visible window exists in the DOM" boundary set
 * `offset`/`size` and re-render.
 */
const virtualWindow = vi.hoisted(() => ({
  offset: 0,
  size: Number.POSITIVE_INFINITY,
}));

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
      getVirtualItems: () => {
        const end = Math.min(items.length, virtualWindow.offset + virtualWindow.size);
        return items.slice(virtualWindow.offset, end);
      },
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
    listTables: (...args: unknown[]) => mockGetTables(...args),
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
    EVENT_CONNECTIONS_CHANGED: 'datazen:connections-changed',
    groupConnections: (connections: ConnectionConfig[], _groups: string[], _query: string) => [
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
const mockRemovePanelsForDatabase = vi.fn();
const mockRemovePanelsForRelation = vi.fn();
vi.mock('../../../stores/panelStore', () => ({
  usePanelStore: Object.assign(
    (sel: (s: typeof panelStoreState) => unknown) => sel(panelStoreState),
    {
      getState: () => ({
        pendingQueryHistoryConnectionId: panelStoreState.pendingQueryHistoryConnectionId,
        setPendingQueryHistory: mockSetPendingQueryHistory,
        removePanelsForDatabase: mockRemovePanelsForDatabase,
        removePanelsForRelation: mockRemovePanelsForRelation,
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
    expect(activeSchema()?.currentDatabase).toBe(dbName);
  });
}

/** Context-menu items are a discriminated union; only `item` entries carry an id + action. */
type MenuActionItem = Extract<NativeMenuItemDef, { kind: 'item' }>;

function findActionItem(items: NativeMenuItemDef[], id: string): MenuActionItem | undefined {
  return items.find((item): item is MenuActionItem => item.kind === 'item' && item.id === id);
}

async function triggerContextMenuAction(
  element: HTMLElement,
  actionId: string,
): Promise<ReturnType<typeof vi.fn>> {
  fireEvent.contextMenu(element);
  const { showWebContextMenu } = await import('../../../stores/contextMenuStore');
  await waitFor(() => {
    const items = vi.mocked(showWebContextMenu).mock.calls.at(-1)?.[0] ?? [];
    expect(items.some((item) => item.kind === 'item' && item.id === actionId)).toBe(true);
  });
  const menuItems = vi.mocked(showWebContextMenu).mock.calls.at(-1)?.[0] ?? [];
  const target = findActionItem(menuItems, actionId);
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

async function triggerCloseDatabase(
  findByText: (text: string) => Promise<HTMLElement>,
  dbName: string,
) {
  await waitFor(() => findByText(dbName));
  await triggerContextMenuAction(
    (await findByText(dbName)).closest('button')!,
    'close-database-connection',
  );
}

async function triggerContextMenuRefresh(element: HTMLElement): Promise<void> {
  const { showWebContextMenu } = await import('../../../stores/contextMenuStore');
  fireEvent.contextMenu(element);
  // The connection-level handler awaits driver command discovery before
  // showing the menu — poll for the refresh item instead of reading syncly.
  await waitFor(() => {
    const items = vi.mocked(showWebContextMenu).mock.calls.at(-1)?.[0] ?? [];
    expect(items.some((item) => item.kind === 'item' && item.id === 'refresh')).toBe(true);
  });
  const menuItems = vi.mocked(showWebContextMenu).mock.calls.at(-1)?.[0] ?? [];
  const refreshItem = findActionItem(menuItems, 'refresh');
  expect(refreshItem).toBeDefined();
  refreshItem?.action?.();
}

async function triggerConnectionRefresh(
  findByText: (text: string) => Promise<HTMLElement>,
  connName = 'Local MySQL',
) {
  const connLabel = await findByText(connName);
  const connRow = connLabel.closest<HTMLElement>('[data-conn-item]')!;
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
  namespaceTree?: SqlNamespace;
  loadedPaths?: Set<string>;
  pathItems?: Record<string, TableInfo[]>;
  loading?: boolean;
  /** Bumped by the store whenever a schema reload invalidates loaded paths. */
  schemaEpoch?: number;
};

function activeSchema() {
  const state = useSchemaStore.getState();
  return state.activeDbSessionId ? state.schemas.get(state.activeDbSessionId) : undefined;
}

/** Patch (or create) one session's schema-cache entry in the real store. */
function seedSessionSchema(dbSessionId: string, patch: SessionSchemaPatch): void {
  useSchemaStore.setState((state) => {
    const base = state.schemas.get(dbSessionId) ?? createEmptyConnectionSchema();
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
  virtualWindow.offset = 0;
  virtualWindow.size = Number.POSITIVE_INFINITY;
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

describe('[tester] navigator object identity keys', () => {
  it('keeps routine overloads unique', () => {
    const routineInteger: UnifiedRow = {
      type: 'object',
      obj: { kind: 'function', schema: 'public', name: 'lookup', signature: 'integer' },
      depth: 0,
      levelDepth: 0,
      catId: 'functions',
      connectionId: 'connection-1',
      dbName: 'database-1',
    };
    const routineText: UnifiedRow = {
      type: 'object',
      obj: { kind: 'function', schema: 'public', name: 'lookup', signature: 'text' },
      depth: 0,
      levelDepth: 0,
      catId: 'functions',
      connectionId: 'connection-1',
      dbName: 'database-1',
    };

    expect(getUnifiedRowKey(routineInteger)).not.toBe(getUnifiedRowKey(routineText));
  });

  it('keeps same-name trigger targets unique', () => {
    const triggerOrders: UnifiedRow = {
      type: 'object',
      obj: {
        kind: 'trigger',
        schema: 'public',
        name: 'audit_trigger',
        targetSchema: 'public',
        targetName: 'orders',
      },
      depth: 0,
      levelDepth: 0,
      catId: 'triggers',
      connectionId: 'connection-1',
      dbName: 'database-1',
    };
    const triggerUsers: UnifiedRow = {
      type: 'object',
      obj: {
        kind: 'trigger',
        schema: 'public',
        name: 'audit_trigger',
        targetSchema: 'public',
        targetName: 'users',
      },
      depth: 0,
      levelDepth: 0,
      catId: 'triggers',
      connectionId: 'connection-1',
      dbName: 'database-1',
    };

    expect(getUnifiedRowKey(triggerOrders)).not.toBe(getUnifiedRowKey(triggerUsers));
  });
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
      expect(activeSchema()?.currentDatabase).toBe('db_b');
    });

    fireEvent.click((await findByText('users')).closest('button')!);

    await waitFor(() => {
      // F1: no use_database IPC — activation only moves the local context.
      expect(activeSchema()?.currentDatabase).toBe('db_a');
    });
    expect(onSelectTable).toHaveBeenCalledWith('users', null, 'db_a');
  });

  it('re-pins currentDatabase when re-expanding an already-cached database', async () => {
    const { findByText, queryAllByText } = render(<ConnectionNavigatorTree {...baseProps} />);

    await ensureDbTableVisible(findByText, queryAllByText, 'db_a', 'users');
    await ensureDbTableVisible(findByText, queryAllByText, 'db_b', 'orders');
    await waitFor(() => {
      expect(activeSchema()?.currentDatabase).toBe('db_b');
    });

    mockGetTables.mockClear();

    // Collapse db_a, then re-expand it: the tables are already cached, so no
    // listTables call fires — yet the pointer must follow the click. The
    // original bug pinned only as a side effect of the fetch, so cache hits
    // left currentDatabase behind and "查看 ER" opened a stale database.
    fireEvent.click((await findByText('db_a')).closest('button')!);
    fireEvent.click((await findByText('db_a')).closest('button')!);
    await waitFor(() => {
      expect(activeSchema()?.currentDatabase).toBe('db_a');
    });
    await findByText('users');
    expect(mockGetTables).not.toHaveBeenCalledWith('conn-1', 'db_a');
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
    fireEvent.click((await findByText('db_a')).closest('button')!);
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

  it('supports dragging table nodes with versioned and legacy drag payloads', async () => {
    const { findByText, queryAllByText } = render(
      <ConnectionNavigatorTree {...baseProps} activeConnectionId="cfg-mysql" />,
    );

    await ensureDbTableVisible(findByText, queryAllByText, 'db_a', 'users');

    const tableButton = (await findByText('users')).closest('button')!;
    expect(tableButton).not.toBeNull();
    expect(tableButton.getAttribute('draggable')).toBe('true');

    const dt = {
      setData: vi.fn(),
      effectAllowed: '',
      dropEffect: '',
    };

    fireEvent.dragStart(tableButton, { dataTransfer: dt });

    expect(dt.setData).toHaveBeenCalledWith('text/plain', 'users');
    expect(dt.setData).toHaveBeenCalledWith(
      'application/datazen-schema-object',
      expect.stringContaining('"table":"users"'),
    );
    expect(dt.setData).toHaveBeenCalledWith(
      'application/datazen-table',
      expect.stringContaining('"tableName":"users"'),
    );
    expect(dt.effectAllowed).toBe('copy');
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

  it('refreshing a background connection keeps the active session focused', async () => {
    connectionsState.connections = [MYSQL_CONN, makeConn({ id: 'cfg-other', name: 'Other MySQL' })];
    activeConnectionsState.connections = {
      'cfg-mysql': { status: 'connected', dbSessionId: 'conn-1', connectionId: 'cfg-mysql' },
      'cfg-other': { status: 'connected', dbSessionId: 'conn-2', connectionId: 'cfg-other' },
    };
    const navigatorRef = createRef<ConnectionNavigatorTreeHandle>();
    const view = render(<ConnectionNavigatorTree {...baseProps} ref={navigatorRef} />);
    await view.findByText('db_a');
    useSchemaStore.getState().setActiveConnection('conn-1');
    mockGetDatabases.mockClear();

    await navigatorRef.current!.refreshConnection('cfg-other');

    // The other session did reload…
    await waitFor(() => {
      expect(mockGetDatabases).toHaveBeenCalledWith('conn-2');
    });
    // …but it must not steal the active session (and with it the flattened
    // top-level currentDatabase that query panels read) from the connection
    // the user is actually working on.
    expect(useSchemaStore.getState().activeDbSessionId).toBe('conn-1');
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
    fireEvent.click((await findByText('db_a')).closest('button')!);
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
      expect(activeSchema()?.currentDatabase).toBe('db_a');
    });

    await triggerDropDatabase(findByText, 'db_a');

    await waitFor(() => {
      expect(activeSchema()?.currentDatabase).toBe('postgres');
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
      expect(activeSchema()?.currentDatabase).toBe('db_b');
    });

    mockGetTables.mockClear();

    await triggerDropDatabase(findByText, 'db_a');

    await waitFor(() => {
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

  it('closes tabs bound to the dropped database after a successful drop', async () => {
    mockRemovePanelsForDatabase.mockClear();
    const { findByText, queryAllByText } = render(<ConnectionNavigatorTree {...baseProps} />);

    await ensureDbTableVisible(findByText, queryAllByText, 'db_a', 'users');
    await waitFor(() => {
      expect(activeSchema()?.currentDatabase).toBe('db_a');
    });

    await triggerDropDatabase(findByText, 'db_a');

    await waitFor(() => {
      expect(mockDriverExecute).toHaveBeenCalledWith({
        dbSessionId: 'conn-1',
        command: 'drop_database',
        input: { name: 'db_a' },
      });
    });
    await waitFor(() => {
      expect(mockRemovePanelsForDatabase).toHaveBeenCalledWith('cfg-mysql', 'db_a', 'db_a');
    });
  });

  it('keeps tabs open when the drop fails', async () => {
    mockRemovePanelsForDatabase.mockClear();
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
    expect(mockRemovePanelsForDatabase).not.toHaveBeenCalled();
  });
});

describe('ConnectionNavigatorTree close database connection', () => {
  it('closes tabs bound to the database after closing its connection', async () => {
    mockRemovePanelsForDatabase.mockClear();
    mockCloseDatabase.mockClear();
    const { findByText, queryAllByText } = render(<ConnectionNavigatorTree {...baseProps} />);

    await ensureDbTableVisible(findByText, queryAllByText, 'db_a', 'users');
    await waitFor(() => {
      expect(activeSchema()?.currentDatabase).toBe('db_a');
    });

    await triggerCloseDatabase(findByText, 'db_a');

    await waitFor(() => {
      expect(mockCloseDatabase).toHaveBeenCalledWith('conn-1', 'db_a');
    });
    await waitFor(() => {
      expect(mockRemovePanelsForDatabase).toHaveBeenCalledWith('cfg-mysql', 'db_a', 'db_a');
    });
  });

  it('keeps tabs open when closing the database connection fails', async () => {
    mockRemovePanelsForDatabase.mockClear();
    mockCloseDatabase.mockRejectedValueOnce(new Error('release failed'));
    const onShowMessage = vi.fn();
    const { findByText } = render(
      <ConnectionNavigatorTree {...baseProps} onShowMessage={onShowMessage} />,
    );

    await triggerCloseDatabase(findByText, 'db_a');

    await waitFor(() => {
      expect(onShowMessage).toHaveBeenCalledWith('release failed', 'error');
    });
    expect(mockRemovePanelsForDatabase).not.toHaveBeenCalled();
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
      expect(activeSchema()?.currentDatabase).toBe('db_b');
    });

    await triggerContextMenuAction((await findByText('db_a')).closest('button')!, 'new-query');

    expect(activeSchema()?.currentDatabase).toBe('db_a');
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
    fireEvent.click((await findByText('db_a')).closest('button')!);
    await waitFor(() => expect(mockGetTables).toHaveBeenCalledWith('conn-1', 'db_a'));
    fireEvent.click((await findByText('public')).closest('button')!);
    await waitFor(() => {
      expect(activeSchema()?.currentDatabase).toBe('db_a');
    });

    // Set a different active database to verify the fix
    seedSessionSchema('conn-1', { currentDatabase: 'db_b' });

    await triggerContextMenuAction((await findByText('public')).closest('button')!, 'new-query');

    expect(activeSchema()?.currentDatabase).toBe('db_a');
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

  it('allows the recent section to collapse and expand', async () => {
    const recent = makeConn({
      id: 'cfg-recent',
      name: 'Recent Conn',
      group: 'Group A',
      lastConnectedAt: '2026-08-31T10:00:00Z',
    });
    connectionsState.connections = [recent];
    connectionsState.groups = ['Group A'];

    const { container } = render(
      <ConnectionNavigatorTree {...baseProps} activeConnectionId={null} />,
    );

    const recentHeader = () =>
      container.querySelector<HTMLButtonElement>('[data-section-header][data-section="recent"]');
    const recentRow = () =>
      container.querySelector('[data-conn-group="__recent__"][data-conn-name="Recent Conn"]');

    await waitFor(() => {
      expect(recentHeader()).not.toBeNull();
      expect(recentRow()).not.toBeNull();
      expect(recentHeader()?.getAttribute('aria-expanded')).toBe('true');
    });

    fireEvent.click(recentHeader()!);
    await waitFor(() => {
      expect(recentHeader()?.getAttribute('aria-expanded')).toBe('false');
      expect(recentRow()).toBeNull();
    });

    fireEvent.click(recentHeader()!);
    await waitFor(() => {
      expect(recentHeader()?.getAttribute('aria-expanded')).toBe('true');
      expect(recentRow()).not.toBeNull();
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

  it('does not render insertion indicator in recent section when dragging over a connection in regular group', async () => {
    const recentConn = makeConn({
      id: 'cfg-recent',
      name: 'Recent Conn',
      group: 'Group A',
      lastConnectedAt: '2026-08-31T10:00:00Z',
    });
    const movingConn = makeConn({ id: 'cfg-moving', name: 'Moving Conn', group: 'Group A' });
    connectionsState.connections = [recentConn, movingConn];
    connectionsState.groups = ['Group A'];

    const { container } = render(<ConnectionNavigatorTree {...baseProps} />);
    await waitFor(() => {
      expect(
        container.querySelector('[data-conn-group="__recent__"][data-conn-name="Recent Conn"]'),
      ).not.toBeNull();
      expect(
        container.querySelector('[data-conn-group="Group A"][data-conn-name="Recent Conn"]'),
      ).not.toBeNull();
    });

    const dt = dataTransferStub();
    const movingRow = container.querySelector(
      '[data-conn-group="Group A"][data-conn-name="Moving Conn"]',
    )!;
    const targetRowInGroupA = container.querySelector(
      '[data-conn-group="Group A"][data-conn-name="Recent Conn"]',
    )!;
    const recentRow = container.querySelector(
      '[data-conn-group="__recent__"][data-conn-name="Recent Conn"]',
    )!;

    // Start dragging Moving Conn
    fireEvent.dragStart(movingRow, { dataTransfer: dt });

    // Drag over Recent Conn in Group A (before)
    targetRowInGroupA.getBoundingClientRect = vi.fn().mockReturnValue({ top: 100, height: 20 });
    fireEvent.dragOver(targetRowInGroupA, { dataTransfer: dt, clientY: 105 });

    // The indicator should only be present in Group A, NOT in recent section
    const groupAWrapper = targetRowInGroupA.parentElement!;
    const recentWrapper = recentRow.parentElement!;

    expect(groupAWrapper.querySelector('.bg-accent')).not.toBeNull();
    expect(recentWrapper.querySelector('.bg-accent')).toBeNull();
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
        { name: 'settings', tableType: 'table', schema: undefined },
        { name: 'v_app', tableType: 'view', schema: undefined },
        { name: 'idx_log', tableType: 'systemTable', schema: undefined },
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
      [{ name: 'settings', tableType: 'table', schema: undefined }],
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

  it('[tester] dispatches routine signatures and trigger relation identity', async () => {
    const openObject = vi.fn();
    const { container, findByText } = await renderWithSqlite(
      [{ name: 'settings', tableType: 'table', schema: null }],
      {},
      { viewActions: { openObject } },
    );
    await findByText('settings');

    mockGetDatabaseObjects.mockImplementation((_c: string, catId: string) => {
      if (catId === 'function') {
        return Promise.resolve([
          {
            name: 'lookup',
            kind: 'function',
            schema: 'public',
            signature: 'integer',
          },
          { name: 'lookup', kind: 'function', schema: 'public', signature: 'text' },
        ]);
      }
      if (catId === 'trigger') {
        return Promise.resolve([
          {
            name: 'audit_trigger',
            kind: 'trigger',
            schema: null,
            targetSchema: null,
            targetName: 'orders',
          },
        ]);
      }
      return Promise.resolve([]);
    });

    fireEvent.click(categoryButton(container, 'function'));
    await waitFor(() => {
      expect(container.querySelectorAll('[data-item-name="lookup"]').length).toBe(2);
    });
    const routines = container.querySelectorAll('[data-item-name="lookup"]');
    fireEvent.click(routines[0]!);
    expect(openObject).toHaveBeenCalledWith('function', 'lookup', 'public', 'integer');
    fireEvent.click(routines[1]!);
    expect(openObject).toHaveBeenCalledWith('function', 'lookup', 'public', 'text');

    fireEvent.click(categoryButton(container, 'trigger'));
    await waitFor(() => {
      expect(container.querySelector('[data-item-name="audit_trigger"]')).not.toBeNull();
    });
    fireEvent.click(container.querySelector('[data-item-name="audit_trigger"]')!);
    expect(openObject).toHaveBeenCalledWith(
      'trigger',
      'audit_trigger',
      undefined,
      undefined,
      undefined,
      'orders',
    );
  });

  it('caches an empty list when an object category fails to refresh', async () => {
    const { container, findByText } = await renderWithSqlite(
      [{ name: 'settings', tableType: 'table', schema: undefined }],
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
      [{ name: 'settings', tableType: 'table', schema: undefined }],
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
    expect(onSelectTable).toHaveBeenCalledWith('settings', null, '/data/app.db');
  });

  it('refresh paths reload expanded categories and single-db tables', async () => {
    const { container, findByText } = await renderWithSqlite(
      [{ name: 'settings', tableType: 'table', schema: undefined }],
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
      [{ name: 'settings', tableType: 'table', schema: undefined }],
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
      [{ name: 'settings', tableType: 'table', schema: undefined }],
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

  it('keeps marking a database node as open while the backend still holds its pool', async () => {
    connectionsState.connections = [
      makeConn({ id: 'cfg-pg', name: 'Local PG', databaseType: 'postgresql', port: 5432 }),
    ];
    activeConnectionsState.connections = {
      'cfg-pg': { status: 'connected', dbSessionId: 'conn-1', connectionId: 'cfg-pg' },
    };
    mockGetTables.mockResolvedValue([]);
    mockGetOpenDatabases.mockResolvedValue(['db_a']);

    const { container, findByText } = render(
      <ConnectionNavigatorTree {...baseProps} activeConnectionId="cfg-pg" />,
    );
    await findByText('db_a');
    await settleSessionLoad('conn-1');

    // Expand then collapse: the node is no longer expanded, so the only thing
    // that can keep the marker is the driver still holding a releasable pool.
    fireEvent.click((await findByText('db_a')).closest('button')!);
    await waitFor(() => {
      expect(mockGetOpenDatabases).toHaveBeenCalledWith('conn-1');
    });
    fireEvent.click((await findByText('db_a')).closest('button')!);
    await waitFor(() => {
      expect(
        container
          .querySelector('[data-db-name="db_a"] [data-db-open]')!
          .getAttribute('data-db-open'),
      ).toBe('true');
    });
    // A database neither expanded nor reported by the driver stays bare.
    expect(container.querySelector('[data-db-name="db_b"] [data-db-open]')).toBeNull();
  });

  it('shows no marker at all once the backend released the pool', async () => {
    connectionsState.connections = [
      makeConn({ id: 'cfg-pg', name: 'Local PG', databaseType: 'postgresql', port: 5432 }),
    ];
    activeConnectionsState.connections = {
      'cfg-pg': { status: 'connected', dbSessionId: 'conn-1', connectionId: 'cfg-pg' },
    };
    mockGetTables.mockResolvedValue([]);
    // The driver reports nothing open (e.g. the pool was just released).
    mockGetOpenDatabases.mockResolvedValue([]);

    const { container, findByText } = render(
      <ConnectionNavigatorTree {...baseProps} activeConnectionId="cfg-pg" />,
    );
    await findByText('db_a');
    await settleSessionLoad('conn-1');

    fireEvent.click((await findByText('db_a')).closest('button')!);
    await waitFor(() => {
      expect(mockGetOpenDatabases).toHaveBeenCalledWith('conn-1');
    });
    fireEvent.click((await findByText('db_a')).closest('button')!);

    // A closed database renders nothing: the old hollow ring repeated the
    // absence of a dot while implying the tree knew the pool state.
    await waitFor(() => {
      expect(container.querySelector('[data-db-name="db_a"] [data-db-open]')).toBeNull();
    });
  });

  it('marks a database node as open as soon as the user expands it', async () => {
    connectionsState.connections = [
      makeConn({ id: 'cfg-pg', name: 'Local PG', databaseType: 'postgresql', port: 5432 }),
    ];
    activeConnectionsState.connections = {
      'cfg-pg': { status: 'connected', dbSessionId: 'conn-1', connectionId: 'cfg-pg' },
    };
    mockGetTables.mockResolvedValue([]);
    // Drivers with no releasable per-database resource report nothing, so the
    // tree's own expand state must be enough to show a database as open.
    mockGetOpenDatabases.mockResolvedValue([]);

    const { container, findByText } = render(
      <ConnectionNavigatorTree {...baseProps} activeConnectionId="cfg-pg" />,
    );
    await findByText('db_a');
    await settleSessionLoad('conn-1');

    expect(container.querySelector('[data-db-name="db_a"] [data-db-open]')).toBeNull();

    fireEvent.click((await findByText('db_a')).closest('button')!);
    await waitFor(() => {
      expect(
        container
          .querySelector('[data-db-name="db_a"] [data-db-open]')!
          .getAttribute('data-db-open'),
      ).toBe('true');
    });

    // Collapsing it closes it again in the tree.
    fireEvent.click((await findByText('db_a')).closest('button')!);
    await waitFor(() => {
      expect(container.querySelector('[data-db-name="db_a"] [data-db-open]')).toBeNull();
    });
  });

  it('shows no open marker when the driver reports no per-database resources', async () => {
    connectionsState.connections = [
      makeConn({ id: 'cfg-pg', name: 'Local PG', databaseType: 'postgresql', port: 5432 }),
    ];
    activeConnectionsState.connections = {
      'cfg-pg': { status: 'connected', dbSessionId: 'conn-1', connectionId: 'cfg-pg' },
    };
    // A failed/absent report must not be rendered as "closed" — the tree would
    // be claiming knowledge the driver never provided.
    mockGetOpenDatabases.mockRejectedValue(new Error('unsupported'));

    const { container, findByText } = render(
      <ConnectionNavigatorTree {...baseProps} activeConnectionId="cfg-pg" />,
    );
    await findByText('db_a');
    await settleSessionLoad('conn-1');

    await waitFor(() => {
      expect(container.querySelector('[data-db-name="db_a"]')).not.toBeNull();
    });
    expect(container.querySelector('[data-db-name="db_a"] [data-db-open]')).toBeNull();
  });

  it('marks an auto-expanded single-database node as open', async () => {
    // Single-database connections expand their configured database on connect,
    // so it reads as open even though the driver reports no per-database pool.
    connectionsState.connections = [
      makeConn({
        id: 'cfg-sql',
        name: 'SQLite Conn',
        databaseType: 'sqlite',
        database: '/data/app.db',
      }),
    ];
    activeConnectionsState.connections = {
      'cfg-sql': { status: 'connected', dbSessionId: 'conn-sql', connectionId: 'cfg-sql' },
    };
    mockGetDatabases.mockResolvedValue(['/data/app.db']);
    mockGetTables.mockResolvedValue([]);
    mockGetOpenDatabases.mockResolvedValue([]);

    const { container, findByText } = render(
      <ConnectionNavigatorTree {...baseProps} activeConnectionId="cfg-sql" />,
    );
    await findByText('/data/app.db');
    await settleSessionLoad('conn-sql');

    await waitFor(() => {
      expect(
        container.querySelector('[data-db-name="/data/app.db"] [data-db-open]'),
      ).not.toBeNull();
    });
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
    onSelectTable?: (name: string, schema: string | null, db: string) => void;
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
async function renderPgTree(
  extraProps: Partial<ConnectionNavigatorTreeProps> = {},
  customTables?: TableInfo[],
) {
  connectionsState.connections = [
    makeConn({ id: 'cfg-pg', name: 'PG Conn', databaseType: 'postgresql', port: 5432 }),
  ];
  activeConnectionsState.connections = {
    'cfg-pg': { status: 'connected', dbSessionId: 'conn-pg', connectionId: 'cfg-pg' },
  };
  mockGetDatabases.mockResolvedValue(['db_a']);
  const defaultTables = [
    { name: 'users', tableType: 'table', schema: 'public' },
    { name: 'info_t', tableType: 'table', schema: 'information_schema' },
    { name: 'cat_t', tableType: 'table', schema: 'pg_catalog' },
  ] as TableInfo[];
  mockGetTables.mockImplementation((_c: string, db: string) =>
    db === 'db_a' ? Promise.resolve(customTables ?? defaultTables) : Promise.resolve([]),
  );
  const view = render(
    <ConnectionNavigatorTree {...baseProps} activeConnectionId="cfg-pg" {...extraProps} />,
  );
  fireEvent.click((await view.findByText('db_a')).closest('button')!);
  return view;
}

describe('ConnectionNavigatorTree multi-db tree variants', () => {
  it('sorts schemas with the driver default schema first', async () => {
    const { container } = await renderPgTree({}, [
      { name: 't_zeta', tableType: 'table', schema: 'zeta' },
      { name: 't_pub', tableType: 'table', schema: 'public' },
      { name: 't_alpha', tableType: 'table', schema: 'alpha' },
    ] as TableInfo[]);

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
    // Expand to pick up the table payload.
    fireEvent.click((await findByText('db_a')).closest('button')!);
    await waitFor(() => {
      expect(mockGetTables).toHaveBeenCalledWith('conn-1', 'db_a');
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
            { name: 'real_table', tableType: 'table', schema: undefined },
            { name: 'internal', tableType: 'systemTable', schema: undefined },
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
      expect(items.some((item) => item.kind === 'item' && item.id === 'refresh')).toBe(true);
    });
    const items = vi.mocked(showWebContextMenu).mock.calls.at(-1)?.[0] ?? [];
    findActionItem(items, 'refresh')?.action?.();

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
      expect(mockDriverExecute).toHaveBeenCalledWith({
        dbSessionId: 'conn-1',
        command: 'drop_database',
        input: { name: 'db_a' },
      });
    });
    await waitFor(() => {
      expect(useSchemaStore.getState().schemas.get('conn-1')?.currentDatabase).toBe('postgres');
    });
    expect(activeSchema()?.currentDatabase).toBe('postgres');
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
      expect(activeSchema()?.currentDatabase).toBe('second');
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
    // Explicit database: a reused ER tab must re-bind to the right-clicked one.
    expect(openErDiagram).toHaveBeenCalledWith(undefined, 'db_a');

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
    expect(activeSchema()?.currentDatabase).toBe('db_a');
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
    // Same contract for schema nodes: pass the owning database explicitly.
    expect(openErDiagram).toHaveBeenCalledWith(undefined, 'db_a');

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
      expect(activeSchema()?.currentDatabase).toBe('db_b');
    });
    await activateDatabaseContext(findByText, 'db_a', 'users');
    mockExecuteQuery.mockClear();
    mockRemovePanelsForRelation.mockClear();

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
    await waitFor(() => {
      expect(mockRemovePanelsForRelation).toHaveBeenCalledWith('cfg-mysql', 'orders', 'db_b');
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
    seedSessionSchema('conn-pg', { currentDatabase: 'db_a' });

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
    const header = [...container.querySelectorAll<HTMLElement>('[data-group-header]')].find((el) =>
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
    const header = [...container.querySelectorAll<HTMLElement>('[data-group-header]')].find((el) =>
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
      expect(onSelectTable).toHaveBeenCalledWith('users', null, 'public');
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

  it('keeps the store pointer off namespace fetch paths when a table menu opens a query', async () => {
    const newQuery = vi.fn();
    const { container, findByText } = await renderNamespaceTree({ viewActions: { newQuery } });

    fireEvent.click((await findByText('public')).closest('button')!);
    await waitFor(() => {
      expect(container.querySelector('[data-item-name="users"]')).not.toBeNull();
    });
    expect(activeSchema()?.currentDatabase).toBe('db_x');

    const leafButton = container.querySelector('[data-item-name="users"]')!.closest('button')!;
    await triggerContextMenuAction(leafButton, 'new-query');

    expect(newQuery).toHaveBeenCalled();
    // The leaf's `database` is the namespace fetch path (`public`), not a real
    // database: the store pointer must stay on db_x so ensureNamespacePath
    // keeps prefixing fetches with the right database root.
    expect(activeSchema()?.currentDatabase).toBe('db_x');
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
    expect(onSelectTable).toHaveBeenCalledWith('db0', null, 'db0');
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
    mockConnect.mockClear();

    const { container } = render(<ConnectionNavigatorTree {...baseProps} />);
    await waitFor(() => connRow(container, 'Idle Conn'));

    await openMenuAndPick(connRow(container, 'Idle Conn'), 'query-history');

    // Should open the connection
    expect(baseProps.onSelectConnection).toHaveBeenCalledWith('cfg-idle');
    expect(mockConnect).toHaveBeenCalledWith(expect.objectContaining({ id: 'cfg-idle' }));
    // Should NOT call viewActions.openQueryHistory (actionsRef is null)
    // Instead should store pending intent in panelStore
    expect(mockSetPendingQueryHistory).toHaveBeenCalledWith('cfg-idle');
  });

  it('reloads expanded databases when the schema fingerprint changes', async () => {
    const view = render(<ConnectionNavigatorTree {...baseProps} />);
    const dbBtn = (await view.findByText('db_a')).closest('button')!;
    fireEvent.click(dbBtn);
    await waitFor(() => {
      expect(mockGetTables).toHaveBeenCalledWith('conn-1', 'db_a');
    });
    mockGetTables.mockClear();

    seedSessionSchema('conn-1', { schemaEpoch: 42 });

    await waitFor(() => {
      expect(mockGetTables).toHaveBeenCalledWith('conn-1', 'db_a');
    });
  });

  it('does not auto-expand default database for multi-db connections', async () => {
    mockGetTables.mockClear();
    const { findByText } = render(
      <ConnectionNavigatorTree {...baseProps} activeConnectionId="cfg-mysql" />,
    );
    await findByText('db_a');
    // Databases level is visible, but default database is not auto-expanded:
    expect(mockGetTables).not.toHaveBeenCalled();
  });
});

describe('ConnectionNavigatorTree ARIA tree semantics', () => {
  it('marks the virtualized row container as the tree and keeps every node inside it', async () => {
    const { container } = await renderWithSqlite(
      [{ name: 'settings', tableType: 'table', schema: undefined }],
      {},
      {},
    );

    const tree = await waitFor(() => {
      const el = container.querySelector('[role="tree"]');
      expect(el).not.toBeNull();
      return el!;
    });
    expect(container.querySelectorAll('[role="tree"]')).toHaveLength(1);

    await waitFor(() => {
      expect(container.querySelector('[data-item-name="settings"]')).not.toBeNull();
    });

    // The virtualizer wrapper is structural, so every node it paints must be a
    // treeitem carrying a 1-based level — that is the whole contract here.
    const items = [...tree.querySelectorAll('[role="treeitem"]')];
    expect(items.length).toBeGreaterThan(0);
    for (const item of items) {
      const level = Number(item.getAttribute('aria-level'));
      expect(Number.isInteger(level), item.outerHTML.slice(0, 120)).toBe(true);
      expect(level).toBeGreaterThanOrEqual(1);
    }
  });

  it('numbers a single-db tree from the group header down to the table row', async () => {
    const { container } = await renderWithSqlite(
      [{ name: 'settings', tableType: 'table', schema: undefined }],
      {},
      {},
    );

    await waitFor(() => {
      expect(container.querySelector('[data-item-name="settings"]')).not.toBeNull();
    });

    const levelOf = (selector: string) => {
      const el = container.querySelector(selector);
      expect(el, selector).not.toBeNull();
      expect(el!.getAttribute('role'), selector).toBe('treeitem');
      return el!.getAttribute('aria-level');
    };

    // ARIA levels are 1-based; section/group headers own the depth-1 children.
    expect(levelOf('[data-group-header]')).toBe('1');
    expect(levelOf('[data-conn-item]')).toBe('2');
    expect(levelOf('[data-tree-node="db"]')).toBe('3');
    expect(levelOf('[data-tree-node="category"]')).toBe('4');
    expect(levelOf('[data-item-name="settings"]')).toBe('5');
  });

  it('keeps counting one level per nesting step in a schema-grouped tree', async () => {
    const { container } = await renderPgTree();

    const levelOf = (selector: string) => {
      const el = container.querySelector(selector);
      expect(el, selector).not.toBeNull();
      return el!.getAttribute('aria-level');
    };

    // Schema rows only exist once the database's table payload resolves.
    await waitFor(() => {
      expect(container.querySelector('[data-tree-node="schema"]')).not.toBeNull();
    });

    expect(levelOf('[data-group-header]')).toBe('1');
    expect(levelOf('[data-conn-item]')).toBe('2');
    expect(levelOf('[data-tree-node="db"]')).toBe('3');
    expect(levelOf('[data-tree-node="schema"]')).toBe('4');

    fireEvent.click(container.querySelector('[data-schema-name="public"]')!);
    await waitFor(() => {
      expect(container.querySelector('[data-cat-id="tables"]')).not.toBeNull();
    });
    expect(levelOf('[data-tree-node="category"]')).toBe('5');

    // Schema-scoped categories start collapsed; open one to reach the deepest
    // level the navigator renders.
    fireEvent.click(categoryButton(container, 'tables'));
    await waitFor(() => {
      expect(container.querySelector('[data-item-name="users"]')).not.toBeNull();
    });
    expect(levelOf('[data-item-name="users"]')).toBe('6');
  });

  it('marks the recent section header as a top-level treeitem', async () => {
    const recent = makeConn({
      id: 'cfg-recent',
      name: 'Recent Conn',
      group: 'Group A',
      lastConnectedAt: '2026-08-31T10:00:00Z',
    });
    connectionsState.connections = [recent];
    connectionsState.groups = ['Group A'];
    activeConnectionsState.connections = {};

    const { container } = render(
      <ConnectionNavigatorTree {...baseProps} activeConnectionId={null} />,
    );

    const header = await waitFor(() => {
      const el = container.querySelector('[data-section-header][data-section="recent"]');
      expect(el).not.toBeNull();
      return el!;
    });
    expect(header.getAttribute('role')).toBe('treeitem');
    expect(header.getAttribute('aria-level')).toBe('1');
    expect(header.getAttribute('aria-expanded')).toBe('true');
  });

  it('leaves the pre-existing aria-expanded conditions untouched', async () => {
    const { container } = await renderWithSqlite(
      [{ name: 'settings', tableType: 'table', schema: undefined }],
      {},
      {},
    );

    await waitFor(() => {
      expect(container.querySelector('[data-tree-node="category"]')).not.toBeNull();
    });

    // These rows already announced their expansion state before the tree roles
    // landed; adding role="treeitem" must not change any of those conditions.
    const db = container.querySelector('[data-tree-node="db"]')!;
    expect(db.getAttribute('aria-expanded')).toBe('true');

    const category = container.querySelector('[data-tree-node="category"]')!;
    expect(category.getAttribute('aria-expanded')).toBe('true');

    // The connection chevron keeps its own conditional aria-expanded, and a
    // collapsed category flips it without losing the treeitem role.
    const chevron = container.querySelector('[data-conn-item] button')!;
    expect(chevron.getAttribute('aria-expanded')).toBe('true');

    const views = container.querySelector('[data-cat-id="views"]')!;
    expect(views.getAttribute('aria-level')).toBe('4');
    expect(views.getAttribute('aria-expanded')).toBe('false');
    fireEvent.click(views);
    await waitFor(() => {
      expect(views.getAttribute('aria-expanded')).toBe('true');
    });
  });
});

// ── Independent audit of the ARIA tree contract ────────────────────────────
//
// The suite above lands role="treeitem" / aria-level on the common rows. This
// one covers the variants that suite never renders (object, view, kv-db,
// db-loading, namespace-node) and asserts the tree *structure* rather than
// individual rows: an ARIA tree is only navigable when every item's level has a
// real parent, so the helper below walks the painted rows the way a screen
// reader would.

describe('ConnectionNavigatorTree ARIA tree shape audit', () => {
  /** Read a row, asserting it really is a treeitem. */
  function treeRow(container: HTMLElement, selector: string): HTMLElement {
    const el = container.querySelector<HTMLElement>(selector);
    if (!el) throw new Error(`row not rendered: ${selector}`);
    expect(el.getAttribute('role'), `role for ${selector}`).toBe('treeitem');
    return el;
  }

  function levelOf(container: HTMLElement, selector: string): number {
    const raw = treeRow(container, selector).getAttribute('aria-level');
    expect(raw, `aria-level for ${selector}`).not.toBeNull();
    return Number(raw);
  }

  /**
   * Walk the painted treeitems in DOM order the way a screen reader builds its
   * parent stack, and report every item whose level has no matching parent.
   * `1 → 3` is a hole in the tree: the reader is told about a level-2 parent
   * that is not in the tree at all.
   */
  function findLevelGaps(container: HTMLElement): string[] {
    const openAncestors: number[] = [];
    const gaps: string[] = [];
    for (const item of container.querySelectorAll<HTMLElement>('[role="treeitem"]')) {
      const level = Number(item.getAttribute('aria-level'));
      const label =
        item.getAttribute('data-item-name') ??
        item.getAttribute('data-conn-name') ??
        item.getAttribute('data-cat-id') ??
        item.getAttribute('data-db-name') ??
        item.getAttribute('data-schema-name') ??
        item.getAttribute('data-tree-node') ??
        item.getAttribute('data-section') ??
        item.getAttribute('data-group-name') ??
        item.textContent?.trim().slice(0, 20) ??
        '?';
      while (openAncestors.length && openAncestors[openAncestors.length - 1] >= level) {
        openAncestors.pop();
      }
      if (level > 1 && openAncestors[openAncestors.length - 1] !== level - 1) {
        gaps.push(
          `level ${level} ("${label}") has no level ${level - 1} ancestor ` +
            `(open ancestors: [${openAncestors.join(', ')}])`,
        );
      }
      openAncestors.push(level);
    }
    return gaps;
  }

  function treeOf(container: HTMLElement): HTMLElement {
    const el = container.querySelector<HTMLElement>('[role="tree"]');
    if (!el) throw new Error('no [role="tree"] container');
    return el;
  }

  /**
   * The `db-loading` row is the only treeitem that renders the loading copy and
   * carries no `data-*` identity of its own (a loading db row also spins, so
   * matching on `.animate-spin` would pick the wrong node).
   */
  function loadingTreeItem(container: HTMLElement): HTMLElement {
    const el = [...container.querySelectorAll<HTMLElement>('[role="treeitem"]')].find(
      (item) =>
        item.textContent?.includes('common.loading') &&
        !item.hasAttribute('data-tree-node') &&
        !item.hasAttribute('data-conn-item'),
    );
    if (!el) throw new Error('db-loading treeitem not rendered');
    return el;
  }

  it('covers the object-row variant: functions and triggers are level-5 treeitems', async () => {
    const { container, findByText } = await renderWithSqlite(
      [{ name: 'settings', tableType: 'table', schema: undefined }],
      {},
      {},
    );
    await findByText('settings');

    mockGetDatabaseObjects.mockImplementation((_connId: string, catId: string) =>
      catId === 'function'
        ? Promise.resolve([{ name: 'fn_calc', kind: 'function' }])
        : Promise.resolve([]),
    );
    fireEvent.click(categoryButton(container, 'function'));
    await waitFor(() => {
      expect(container.querySelector('[data-item-name="fn_calc"]')).not.toBeNull();
    });

    // SQLite is single-db with no schema grouping, so the chain is
    // group → connection → db → category(function) → object.
    expect(levelOf(container, '[data-group-header]')).toBe(1);
    expect(levelOf(container, '[data-conn-item]')).toBe(2);
    expect(levelOf(container, '[data-tree-node="db"]')).toBe(3);
    expect(levelOf(container, '[data-cat-id="function"]')).toBe(4);
    expect(levelOf(container, '[data-item-name="fn_calc"]')).toBe(5);
    expect(findLevelGaps(container)).toEqual([]);
  });

  it('covers the view variant: a materialized view is a leaf table treeitem', async () => {
    const { container } = await renderWithSqlite(
      [
        { name: 'settings', tableType: 'table', schema: undefined },
        { name: 'v_report', tableType: 'view', schema: undefined },
      ] as TableInfo[],
      {},
      {},
    );
    // Categories render collapsed, so open Views to reach the leaf rows.
    await waitFor(() => {
      expect(categoryButton(container, 'views')).toBeTruthy();
    });
    fireEvent.click(categoryButton(container, 'views'));
    await waitFor(() => {
      expect(container.querySelector('[data-tree-node="view"]')).not.toBeNull();
    });

    // Views share the table render path but flip data-tree-node to "view".
    const viewRow = treeRow(container, '[data-tree-node="view"]');
    expect(viewRow.getAttribute('data-item-name')).toBe('v_report');
    expect(viewRow.getAttribute('aria-level')).toBe('5');
    expect(levelOf(container, '[data-cat-id="views"]')).toBe(4);
    expect(findLevelGaps(container)).toEqual([]);
  });

  it('covers the kv-db variant: key-value databases hang off the connection at level 3', async () => {
    connectionsState.connections = [
      makeConn({ id: 'cfg-kv', name: 'Redis Conn', databaseType: 'redis' }),
    ];
    activeConnectionsState.connections = {
      'cfg-kv': { status: 'connected', dbSessionId: 'conn-kv', connectionId: 'cfg-kv' },
    };
    mockGetDatabases.mockResolvedValue(['db0', 'db1']);

    const { container, findByText } = render(
      <ConnectionNavigatorTree {...baseProps} activeConnectionId="cfg-kv" />,
    );
    await findByText('db0');

    expect(levelOf(container, '[data-group-header]')).toBe(1);
    expect(levelOf(container, '[data-conn-item]')).toBe(2);
    expect(levelOf(container, '[data-tree-node="kv-db"]')).toBe(3);
    // A key-value store has no categories, so the whole tree stops at level 3.
    expect(container.querySelector('[data-tree-node="category"]')).toBeNull();
    expect(findLevelGaps(container)).toEqual([]);
  });

  it('covers the db-loading variant at both places buildFlatRows emits it', async () => {
    // (a) depth 3 — a database is still fetching its tables.
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

    const { container, findByText } = render(
      <ConnectionNavigatorTree {...baseProps} activeConnectionId="cfg-sql" />,
    );
    await findByText('common.loading');

    // The db row spins too, so pick the treeitem that actually carries the
    // loading copy rather than the first .animate-spin in the subtree.
    const placeholder = loadingTreeItem(container);
    expect(levelOf(container, '[data-tree-node="db"]')).toBe(3);
    expect(placeholder.getAttribute('aria-level')).toBe('4');
    expect(findLevelGaps(container)).toEqual([]);
  });

  it('covers the db-loading variant emitted for a key-value store at depth 2', async () => {
    connectionsState.connections = [
      makeConn({ id: 'cfg-kv', name: 'Redis Conn', databaseType: 'redis' }),
    ];
    activeConnectionsState.connections = {
      'cfg-kv': { status: 'connecting', dbSessionId: '', connectionId: 'cfg-kv' },
    };
    mockGetDatabases.mockResolvedValue([]);

    const { container, findByText } = render(
      <ConnectionNavigatorTree {...baseProps} activeConnectionId="cfg-kv" />,
    );
    await findByText('common.loading');

    // isConnecting short-circuits before any database is known, so this
    // placeholder is a direct child of the connection row.
    expect(levelOf(container, '[data-conn-item]')).toBe(2);
    expect(loadingTreeItem(container).getAttribute('aria-level')).toBe('3');
  });

  it('covers the namespace-node variant: branches and typed leaves keep counting', async () => {
    connectionsState.connections = [
      makeConn({ id: 'cfg-doris', name: 'Doris Conn', databaseType: 'doris' }),
    ];
    activeConnectionsState.connections = {
      'cfg-doris': { status: 'connected', dbSessionId: 'conn-doris', connectionId: 'cfg-doris' },
    };
    mockGetDatabases.mockResolvedValue(['db_x']);
    const { container, findByText } = render(
      <ConnectionNavigatorTree {...baseProps} activeConnectionId="cfg-doris" />,
    );
    await settleSessionLoad('conn-doris');
    seedSessionSchema('conn-doris', {
      currentDatabase: 'db_x',
      namespaceTree: { public: { users: [] }, mv1: [] },
      tables: [
        { name: 'mv1', tableType: 'materializedView' },
        { name: 'users', tableType: 'table' },
      ] as TableInfo[],
      loadedPaths: new Set<string>(),
      pathItems: {},
    });
    await findByText('public');

    // Top-level namespace nodes start at baseDepth 2 under the connection.
    expect(levelOf(container, '[data-group-header]')).toBe(1);
    expect(levelOf(container, '[data-conn-item]')).toBe(2);
    expect(levelOf(container, '[data-tree-node="namespace"]')).toBe(3);
    // A top-level materialized view is a namespace leaf, not a category leaf.
    expect(levelOf(container, '[data-tree-node="view"]')).toBe(3);

    fireEvent.click((await findByText('public')).closest('button')!);
    await waitFor(() => {
      expect(container.querySelector('[data-item-name="users"]')).not.toBeNull();
    });
    expect(levelOf(container, '[data-item-name="users"]')).toBe(4);
    expect(findLevelGaps(container)).toEqual([]);
  });

  it('keeps the two non-node variants out of the tree', async () => {
    // `no-connections` — the whole tree is a single placeholder block.
    connectionsState.connections = [];
    connectionsState.groups = [];
    activeConnectionsState.connections = {};
    const empty = render(<ConnectionNavigatorTree {...baseProps} activeConnectionId={null} />);
    await empty.findByText('main.noConnections');
    expect(empty.container.querySelector('[role="treeitem"]')).toBeNull();
    empty.unmount();

    // `empty-group` — a hint row under an expanded but childless group header.
    connectionsState.connections = [makeConn({ id: 'cfg-x', name: 'Lonely', group: 'Group Z' })];
    connectionsState.groups = ['Group Z', 'Group Y'];
    activeConnectionsState.connections = {};
    const hint = render(<ConnectionNavigatorTree {...baseProps} activeConnectionId={null} />);
    // Every group is expanded on mount (ConnectionNavigatorTree seeds
    // expandedGroups from the group list), so the childless group paints its
    // hint row straight away.
    const groupHeader = (await hint.findByText('Group Y')).closest('[data-group-header]')!;
    expect(groupHeader.getAttribute('role')).toBe('treeitem');
    expect(groupHeader.getAttribute('aria-level')).toBe('1');

    await waitFor(() => {
      expect(hint.container.querySelector('[data-empty-group]')).not.toBeNull();
    });
    const emptyHint = hint.container.querySelector('[data-empty-group]')!;
    expect(emptyHint.getAttribute('data-empty-group')).toBe('Group Y');
    expect(emptyHint.getAttribute('role')).toBeNull();
    expect(emptyHint.getAttribute('aria-level')).toBeNull();
  });

  it('never nests a group inside a section: every header is a level-1 treeitem', async () => {
    const recent = makeConn({
      id: 'cfg-recent',
      name: 'Recent Conn',
      group: 'Group A',
      lastConnectedAt: '2026-08-31T10:00:00Z',
    });
    const a1 = makeConn({ id: 'cfg-a1', name: 'A One', group: 'Group A' });
    const a2 = makeConn({ id: 'cfg-a2', name: 'A Two', group: 'Group A' });
    const b1 = makeConn({ id: 'cfg-b1', name: 'B One', group: 'Group B' });
    connectionsState.connections = [recent, a1, a2, b1];
    connectionsState.groups = ['Group A', 'Group B'];
    activeConnectionsState.connections = {};

    const { container, findByText } = render(
      <ConnectionNavigatorTree {...baseProps} activeConnectionId={null} />,
    );
    await findByText('A One');
    await findByText('B One');

    // buildFlatRows pushes at most one header per top-level section, and it is
    // a `section` XOR a `group` — so groups are siblings of sections, never
    // children. Both therefore sit at level 1.
    const sectionHeader = container.querySelector('[data-section-header][data-section="recent"]')!;
    expect(sectionHeader.getAttribute('aria-level')).toBe('1');

    const groupHeaders = [...container.querySelectorAll('[data-group-header]')];
    expect(groupHeaders.map((g) => g.getAttribute('data-group-name')).sort()).toEqual([
      'Group A',
      'Group B',
    ]);
    for (const header of groupHeaders) {
      expect(header.getAttribute('aria-level')).toBe('1');
    }

    // The recent connection is listed twice — once under the section, once
    // under its own group — and both copies are level-2 children of a header.
    const connNames = [...container.querySelectorAll('[data-conn-item]')].map((c) =>
      c.getAttribute('data-conn-name'),
    );
    expect(connNames).toContain('Recent Conn');
    expect(connNames.filter((n) => n === 'Recent Conn')).toHaveLength(2);
    for (const conn of container.querySelectorAll('[data-conn-item]')) {
      expect(conn.getAttribute('aria-level')).toBe('2');
    }
    expect(findLevelGaps(container)).toEqual([]);
  });

  it('keeps aria-level a property of the row, not of where the virtualizer paints it', async () => {
    const { container, rerender } = await renderWithSqlite(
      [
        { name: 'settings', tableType: 'table', schema: undefined },
        { name: 'audit_log', tableType: 'table', schema: undefined },
        { name: 'v_report', tableType: 'view', schema: undefined },
      ] as TableInfo[],
      {},
      {},
    );
    // Row order once both categories are open:
    // 0 group, 1 connection, 2 db, 3 Tables, 4 settings, 5 audit_log,
    // 6 Views, 7 v_report.
    // Tables is the auto-expanded default category; Views is not.
    await waitFor(() => {
      expect(container.querySelector('[data-item-name="settings"]')).not.toBeNull();
    });
    fireEvent.click(categoryButton(container, 'views'));
    await waitFor(() => {
      expect(container.querySelector('[data-item-name="v_report"]')).not.toBeNull();
    });

    const levelByName = () =>
      new Map(
        [...container.querySelectorAll<HTMLElement>('[role="treeitem"][data-item-name]')].map(
          (el) => [el.getAttribute('data-item-name'), el.getAttribute('aria-level')],
        ),
      );

    const fullWindow = levelByName();
    expect(fullWindow.get('settings')).toBe('5');
    expect(fullWindow.get('audit_log')).toBe('5');
    expect(fullWindow.get('v_report')).toBe('5');

    // Scroll past the header, the connection and the db row: only the two
    // table leaves stay in the DOM, and they must still report level 5 even
    // though their level-1..4 ancestors are no longer painted.
    virtualWindow.offset = 4;
    virtualWindow.size = 2;
    rerender(
      <ConnectionNavigatorTree
        {...baseProps}
        activeConnectionId="cfg-sql"
        onSelectTable={baseProps.onSelectTable}
      />,
    );

    expect(container.querySelector('[data-group-header]')).toBeNull();
    expect(container.querySelector('[data-conn-item]')).toBeNull();
    expect(container.querySelector('[data-tree-node="db"]')).toBeNull();

    const windowed = levelByName();
    expect([...windowed.keys()].sort()).toEqual(['audit_log', 'settings']);
    for (const [name, level] of windowed) {
      expect(level, `level for ${name} after scrolling`).toBe(fullWindow.get(name));
    }
  });

  it('describes the virtualization role shape: one tree, role-less row wrappers', async () => {
    const { container } = await renderWithSqlite(
      [{ name: 'settings', tableType: 'table', schema: undefined }],
      {},
      {},
    );
    await waitFor(() => {
      expect(container.querySelector('[data-item-name="settings"]')).not.toBeNull();
    });

    const tree = treeOf(container);
    expect(container.querySelectorAll('[role="tree"]')).toHaveLength(1);

    // Every treeitem is a descendant of the single tree container, and the
    // tree's direct children are the absolutely-positioned virtualizer
    // wrappers, which stay generic divs (they contain focusable buttons, so
    // role="presentation"/"none" would be an ARIA anti-pattern).
    const directChildren = [...tree.children];
    expect(directChildren.length).toBeGreaterThan(0);
    for (const child of directChildren) {
      expect(child.getAttribute('role')).toBeNull();
      expect(child.querySelectorAll('[role="treeitem"]').length).toBeGreaterThan(0);
    }
    expect(tree.querySelectorAll('[role="treeitem"]').length).toBe(
      directChildren.reduce(
        (sum, child) => sum + child.querySelectorAll('[role="treeitem"]').length,
        0,
      ),
    );
  });

  // ── Level and expansion state contracts ───────────────────────────────
  //
  // Every treeitem that owns children has to announce that state: a level the
  // reader can resolve to a real parent, and an aria-expanded the reader can
  // hear change. These were `it.fails` while the DOM did not meet them.

  it('search results must not skip a level: a db is one level under its connection', async () => {
    const { container, findByText } = await renderWithSqlite(
      [{ name: 'settings', tableType: 'table', schema: undefined }],
      {},
      {},
    );
    await findByText('settings');

    // Searching drops the section/group header (buildFlatRows only emits one
    // when `!query`) and moves connections to depth 0, while databases stay
    // painted at depth 2. The reported level follows the logical parent
    // rather than that indent, so the connection owns the database directly.
    fireEvent.change(searchInput(container), { target: { value: 'SQLite' } });
    await waitFor(() => {
      expect(container.querySelector('[data-group-header]')).toBeNull();
    });
    expect(levelOf(container, '[data-conn-item]')).toBe(1);
    expect(container.querySelector('[data-tree-node="db"]')).not.toBeNull();

    expect(findLevelGaps(container)).toEqual([]);
  });

  it('a group header is a treeitem with children, so it must expose aria-expanded', async () => {
    connectionsState.connections = [makeConn({ id: 'cfg-a1', name: 'A One', group: 'Group A' })];
    connectionsState.groups = ['Group A'];
    activeConnectionsState.connections = {};

    const { findByText } = render(
      <ConnectionNavigatorTree {...baseProps} activeConnectionId={null} />,
    );
    const header = (await findByText('Group A')).closest('[data-group-header]')!;
    expect(header.getAttribute('aria-level')).toBe('1');
    // Groups are seeded expanded on mount (ConnectionNavigatorTree expands
    // every known group on the first effect), so the contract to pin is that
    // the attribute exists and tracks the toggle, not a fixed initial value.
    expect(header.getAttribute('aria-expanded')).toBe('true');

    fireEvent.click(header);
    await waitFor(() => {
      expect(document.querySelector('[data-group-header]')?.getAttribute('aria-expanded')).toBe(
        'false',
      );
    });
  });

  it('a connection treeitem must carry aria-expanded itself, not only its chevron button', async () => {
    const { container, findByText } = await renderWithSqlite(
      [{ name: 'settings', tableType: 'table', schema: undefined }],
      {},
      {},
    );
    await findByText('settings');

    const conn = container.querySelector('[data-conn-item]')!;
    // The chevron <button> inside the row still reports the state...
    expect(conn.querySelector('button')!.getAttribute('aria-expanded')).toBe('true');
    // ...but a screen reader walking the tree reads the treeitem, which is
    // silent, so the node announces as a leaf.
    expect(conn.getAttribute('aria-expanded')).toBe('true');
  });

  it('an expanded namespace branch is a treeitem with children and needs aria-expanded', async () => {
    connectionsState.connections = [
      makeConn({ id: 'cfg-doris', name: 'Doris Conn', databaseType: 'doris' }),
    ];
    activeConnectionsState.connections = {
      'cfg-doris': { status: 'connected', dbSessionId: 'conn-doris', connectionId: 'cfg-doris' },
    };
    mockGetDatabases.mockResolvedValue(['db_x']);
    const { container, findByText } = render(
      <ConnectionNavigatorTree {...baseProps} activeConnectionId="cfg-doris" />,
    );
    await settleSessionLoad('conn-doris');
    seedSessionSchema('conn-doris', {
      currentDatabase: 'db_x',
      namespaceTree: { public: { users: [] } },
      tables: [] as TableInfo[],
      loadedPaths: new Set<string>(),
      pathItems: {},
    });
    await findByText('public');

    // The tree is virtualized and recycles row elements by index, so a
    // captured element is not a stable handle on a row: once the click
    // shifts the rows, the same element is re-rendered as something else.
    // Look the branch up by its label, and look it up again afterwards
    // rather than reusing the element captured before the click.
    const branch = (await findByText('public')).closest('[data-tree-node="namespace"]')!;
    expect(branch.getAttribute('aria-expanded')).toBe('false');

    fireEvent.click(branch);
    await waitFor(() => {
      expect(container.querySelector('[data-item-name="users"]')).not.toBeNull();
    });
    // `branch` is still in the document after the click, but it now renders a
    // different row than the one that was clicked — hence the re-resolve.
    const expanded = (await findByText('public')).closest('[data-tree-node="namespace"]')!;
    expect(expanded.getAttribute('aria-expanded')).toBe('true');
  });
});
