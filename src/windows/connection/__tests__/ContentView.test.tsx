import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, screen } from '@testing-library/react';
import type { ReactNode } from 'react';
import type { ConnectionSchemaState } from '../../../stores/schemaStoreState';
import { seedConnectionSchema } from '../../../test/mocks/schemaStore';

// Mock ResizeObserver for useCompactToolbar
class MockResizeObserver {
  observe() {}
  unobserve() {}
  disconnect() {}
}
// eslint-disable-next-line @typescript-eslint/no-unnecessary-condition
globalThis.ResizeObserver ??= MockResizeObserver as unknown as typeof ResizeObserver;

const { getConnectionViewMock, schemaState, tableDataState, MockRedisView } = vi.hoisted(() => {
  const MockRedisView = () => <div data-testid="mock-redis-view">redis</div>;
  return {
    getConnectionViewMock: vi.fn((..._args: unknown[]) => MockRedisView),
    MockRedisView,
    schemaState: {
      schemas: new Map<string, ConnectionSchemaState>(),
      activeDbSessionId: null as string | null,
      loadForConnection: vi.fn(),
      loadTables: vi.fn(),
      removeRelation: vi.fn(),
      setCurrentDatabase: vi.fn(),
    },
    tableDataState: {
      byPanel: new Map<string, unknown>(),
      removePanel: vi.fn(),
      stageCellChange: vi.fn(),
      applyColumnToRows: vi.fn(),
      invalidateCachedData: vi.fn(),
    },
  };
});

function mockDiv(testId: string) {
  return ({ children }: { children?: ReactNode }) => <div data-testid={testId}>{children}</div>;
}

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

const confirmMock = vi.hoisted(() => vi.fn().mockResolvedValue(false));
const showNativeContextMenuMock = vi.hoisted(() =>
  vi.fn((...args: unknown[]) => {
    const items = args[0] as Array<{ id?: string; action?: () => void }>;
    (
      items.find((item) => item.id === 'drop') ?? items.find((item) => item.id === 'drop-view')
    )?.action?.();
  }),
);
const executeQueryMock = vi.hoisted(() => vi.fn().mockResolvedValue({}));

vi.mock('../../../hooks/useConfirmDialog', () => ({
  useConfirmDialog: () => [confirmMock, null],
}));

vi.mock('../../../hooks/useKeyboardShortcuts', () => ({
  useKeyboardShortcuts: vi.fn(),
}));

vi.mock('../../../hooks/useResizable', () => ({
  useResizable: () => ({
    size: 320,
    handleRef: vi.fn(),
  }),
}));

vi.mock('../../../stores/settingsStore', () => ({
  useSettingsStore: (sel: (s: { settings: { safeMode: boolean } }) => unknown) =>
    sel({ settings: { safeMode: false } }),
}));

vi.mock('../../../stores/connectionStore', () => ({
  useConnectionStore: (sel: (s: { connections: unknown[] }) => unknown) => sel({ connections: [] }),
}));

vi.mock('../../../stores/activeConnectionStore', () => ({
  useActiveConnectionStore: Object.assign(
    (sel: (s: { connections: Record<string, unknown> }) => unknown) => sel({ connections: {} }),
    { getState: () => ({ connections: {} }) },
  ),
}));

// The real module is replaced wholesale, so this must expose EVERY value export
// it has — `useConnectionSchemaField` used to be missing, and every render of
// this suite died on `No "useConnectionSchemaField" export is defined on the
// "../../../stores/schemaStore" mock` before a single assertion ran.
vi.mock('../../../stores/schemaStore', async () => {
  const { schemaStoreMockModule } = await import('../../../test/mocks/schemaStore');
  return schemaStoreMockModule(schemaState);
});

vi.mock('../../../stores/tableDataStore', () => ({
  useTableDataStore: Object.assign(
    (sel: (s: typeof tableDataState) => unknown) => sel(tableDataState),
    { getState: () => tableDataState },
  ),
}));

vi.mock('../../../lib/databaseTypes', () => ({
  DB_REGISTRY: {
    postgresql: {
      label: 'PostgreSQL',
      supportsSQL: true,
      readOnly: false,
      supportsErDiagram: true,
      connectionView: 'sql',
    },
    redis: {
      label: 'Redis',
      supportsSQL: false,
      readOnly: true,
      supportsErDiagram: false,
      connectionView: 'keyvalue',
      isKeyValue: true,
    },
  },
  escapeIdent: (name: string) => `"${name}"`,
  getDbLabel: (t: string) => t,
  getDbIcon: () => ({ label: 'PG', bg: 'bg-blue-500' }),
}));

vi.mock('../../../lib/structureEditor/canOpenStructureEditor', () => ({
  canOpenStructureEditor: () => true,
}));

vi.mock('../../../lib/exportCapability', () => ({
  resolveExportScope: () => ({}),
  supportsFullTableExport: () => false,
  supportsAnyExport: () => false,
}));

vi.mock('../../../lib/structureEditor/resolveCreateTableSchema', () => ({
  resolveCreateTableSchema: () => 'public',
}));

vi.mock('../../../lib/schemaCache', () => ({
  getCachedDDL: vi.fn(),
  invalidateSchemaCache: vi.fn(),
}));

vi.mock('../../../lib/nativeContextMenu', () => ({
  showNativeContextMenu: (...args: unknown[]) => showNativeContextMenuMock(...args),
}));

vi.mock('../../../lib/schemaTreeContextMenu', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../../lib/schemaTreeContextMenu')>();
  return actual;
});

vi.mock('../../../lib/connectionTabContextMenu', () => ({
  buildConnectionTabContextMenuItems: vi.fn(() => []),
}));

vi.mock('../../../lib/sqlDialects', () => ({
  getSqlDialect: () => ({
    getTruncateTableSql: (quoted: string) => `TRUNCATE TABLE ${quoted}`,
  }),
}));

vi.mock('../../../commands/query', () => ({
  queryCommands: {
    executeQuery: (...args: unknown[]) => executeQueryMock(...args),
  },
}));

vi.mock('../../../lib/connectionViews', () => ({
  getConnectionView: (...args: unknown[]) => getConnectionViewMock(...args),
}));

vi.mock('../../../lib/connectionViews/types', () => ({}));

vi.mock('../../../lib/windowManager', () => ({
  openDocsWindow: vi.fn(),
  openNewConnectionDialog: vi.fn(),
}));

vi.mock('../../../lib/loadBatchExportTable', () => ({
  loadBatchExportTableData: vi.fn(),
}));

vi.mock('../../../lib/rowToRecord', () => ({
  rowToRecord: vi.fn(),
}));

vi.mock('../../../components/ui/Button', () => ({
  Button: ({
    children,
    onClick,
    ...props
  }: {
    children?: ReactNode;
    onClick?: () => void;
    [key: string]: unknown;
  }) => (
    <button type="button" onClick={onClick} {...props}>
      {children}
    </button>
  ),
}));

vi.mock('../TableView', () => ({ TableView: mockDiv('mock-table-view') }));
vi.mock('../StructureView', () => ({ StructureView: mockDiv('mock-structure-view') }));
vi.mock('../IndexesView', () => ({ IndexesView: mockDiv('mock-indexes-view') }));
vi.mock('../ForeignKeysView', () => ({ ForeignKeysView: mockDiv('mock-foreign-keys-view') }));
vi.mock('../DDLView', () => ({ DDLView: mockDiv('mock-ddl-view') }));
vi.mock('../QueryPanel', () => ({ QueryPanel: mockDiv('mock-query-panel') }));
vi.mock('../ExportDialog', () => ({ ExportDialog: mockDiv('mock-export-dialog') }));
vi.mock('../BatchExportDialog', () => ({ BatchExportDialog: mockDiv('mock-batch-export-dialog') }));
vi.mock('../ImportDialog', () => ({ ImportDialog: mockDiv('mock-import-dialog') }));
vi.mock('../TableStructureEditor', () => ({
  TableStructureEditor: mockDiv('mock-table-structure-editor'),
}));
vi.mock('../ErDiagramView', () => ({ ErDiagramView: mockDiv('mock-er-diagram-view') }));
vi.mock('../ObjectBrowser', () => ({ ObjectBrowser: mockDiv('mock-object-browser') }));
vi.mock('../DatabaseObjectView', () => ({
  DatabaseObjectView: mockDiv('mock-database-object-view'),
}));
vi.mock('../PrivilegeView', () => ({ PrivilegeView: mockDiv('mock-privilege-view') }));
vi.mock('../../../components/DataTable/DetailPanel', () => ({
  DetailPanel: mockDiv('mock-detail-panel'),
}));
vi.mock('../../../components/DataTable/DetailPanelToggle', () => ({
  DetailPanelToggle: mockDiv('mock-detail-panel-toggle'),
}));
vi.mock('../../../components/ai/AiChatPanel', () => ({
  AiChatPanel: mockDiv('mock-ai-chat-panel'),
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

describe('ContentView', () => {
  let ContentView: typeof import('../ContentView').ContentView;
  let panelStore: typeof import('../../../stores/panelStore');

  beforeEach(async () => {
    vi.clearAllMocks();
    getConnectionViewMock.mockImplementation(() => MockRedisView);
    tableDataState.byPanel = new Map();
    schemaState.activeDbSessionId = null;
    schemaState.schemas = new Map();
    panelStore = await import('../../../stores/panelStore');
    panelStore.usePanelStore.setState({ panels: [], activePanelId: null, queryExec: new Map() });
    ({ ContentView } = await import('../ContentView'));
  });

  afterEach(() => {
    cleanup();
  });

  it('shows workspace home when no panels exist', () => {
    render(<ContentView />);
    expect(screen.getByTestId('connection-workspace-home')).toBeInTheDocument();
    expect(screen.getByText('main.noConnections')).toBeInTheDocument();
  });

  it('renders tab bar with panels', () => {
    const panel = {
      connectionId: 'cfg-1',
      dbSessionId: 'conn-1',
      connectionName: 'TestDB',
      databaseType: 'postgresql' as const,
      type: 'table' as const,
      id: 'panel-tbl-1',
      tableName: 'users',
      database: 'appdb',
      tableSchema: 'public',
      subTab: 'data' as const,
    };
    panelStore.usePanelStore.setState({
      panels: [panel],
      activePanelId: panel.id,
    });

    render(<ContentView />);
    expect(screen.getByTestId('panel-tab')).toHaveTextContent('users');
  });

  it('prunes table-data slices whose panel no longer exists', () => {
    tableDataState.byPanel.set('stale-panel', {});
    panelStore.usePanelStore.setState({ panels: [], activePanelId: null });

    render(<ContentView />);
    expect(tableDataState.removePanel).toHaveBeenCalledWith('stale-panel');
  });

  it('shows toolbar buttons for SQL connections', () => {
    const panel = {
      connectionId: 'cfg-1',
      dbSessionId: 'conn-1',
      connectionName: 'TestDB',
      databaseType: 'postgresql' as const,
      type: 'table' as const,
      id: 'panel-tbl-1',
      tableName: 'users',
      database: 'appdb',
      tableSchema: 'public',
      subTab: 'data' as const,
    };
    panelStore.usePanelStore.setState({
      panels: [panel],
      activePanelId: panel.id,
    });

    render(<ContentView />);
    expect(screen.getByRole('button', { name: /common.newQuery/ })).toBeInTheDocument();
  });

  it('hides SQL toolbar buttons when no active panel', () => {
    panelStore.usePanelStore.setState({ panels: [], activePanelId: null });

    render(<ContentView />);
    expect(screen.queryByRole('button', { name: /common.newQuery/ })).not.toBeInTheDocument();
  });

  it('syncs session currentDatabase to the active panel bound database', () => {
    schemaState.setCurrentDatabase.mockClear();
    const panel = {
      connectionId: 'cfg-1',
      dbSessionId: 'conn-1',
      connectionName: 'TestDB',
      databaseType: 'postgresql' as const,
      type: 'query' as const,
      id: 'panel-q-1',
      title: 'Query 1',
      database: 'tradingdb',
      schema: 'public',
    };
    panelStore.usePanelStore.setState({ panels: [panel], activePanelId: panel.id });

    render(<ContentView />);
    expect(schemaState.setCurrentDatabase).toHaveBeenCalledWith('tradingdb', 'conn-1');
  });

  it('renders redis panel via getConnectionView', () => {
    const redisPanel = {
      connectionId: 'cfg-redis',
      dbSessionId: 'conn-redis',
      connectionName: 'Redis',
      databaseType: 'redis' as const,
      type: 'redis-db' as const,
      id: 'panel-redis-1',
      dbName: 'db0',
    };
    panelStore.usePanelStore.setState({
      panels: [redisPanel],
      activePanelId: redisPanel.id,
    });

    render(<ContentView />);

    expect(getConnectionViewMock).toHaveBeenCalledWith('keyvalue');
    expect(screen.getByTestId('mock-redis-view')).toBeInTheDocument();
  });

  it('hides New Query button for redis panels', () => {
    const redisPanel = {
      connectionId: 'cfg-redis',
      dbSessionId: 'conn-redis',
      connectionName: 'Redis',
      databaseType: 'redis' as const,
      type: 'redis-db' as const,
      id: 'panel-redis-1',
      dbName: 'db0',
    };
    panelStore.usePanelStore.setState({
      panels: [redisPanel],
      activePanelId: redisPanel.id,
    });

    render(<ContentView />);
    expect(screen.queryByRole('button', { name: /common.newQuery/ })).not.toBeInTheDocument();
  });

  it('pins sidebar drop SQL to currentDatabase while session may differ', async () => {
    confirmMock.mockResolvedValueOnce(true);
    const nodeContextMenuRef = {
      current: undefined as ((payload: unknown) => void) | undefined,
    };
    const panel = {
      connectionId: 'cfg-1',
      dbSessionId: 'conn-1',
      connectionName: 'TestDB',
      databaseType: 'postgresql' as const,
      type: 'table' as const,
      id: 'panel-tbl-1',
      tableName: 'users',
      database: 'appdb',
      tableSchema: 'public',
      subTab: 'data' as const,
    };
    panelStore.usePanelStore.setState({
      panels: [panel],
      activePanelId: panel.id,
    });
    schemaState.activeDbSessionId = 'conn-1';
    seedConnectionSchema(schemaState, 'conn-1', {
      currentDatabase: 'db_b',
      tables: [{ name: 'users', schema: 'public', tableType: 'table' }],
    });

    render(<ContentView nodeContextMenuRef={nodeContextMenuRef} />);
    expect(nodeContextMenuRef.current).toBeTypeOf('function');

    nodeContextMenuRef.current?.({
      kind: 'table',
      name: 'users',
      schema: 'public',
      x: 0,
      y: 0,
    });

    await vi.waitFor(() => {
      expect(executeQueryMock).toHaveBeenCalledWith(
        'conn-1',
        'DROP TABLE "users"',
        undefined,
        'db_b',
        'public',
      );
    });
  });

  it('closes view panels when dropping a view from the context menu', async () => {
    confirmMock.mockResolvedValueOnce(true);
    const nodeContextMenuRef = {
      current: undefined as ((payload: unknown) => void) | undefined,
    };
    const viewPanel = {
      connectionId: 'cfg-1',
      dbSessionId: 'conn-1',
      connectionName: 'TestDB',
      databaseType: 'postgresql' as const,
      type: 'view' as const,
      id: 'panel-view-1',
      viewName: 'v_users',
      database: 'appdb',
      viewSchema: 'public',
      subTab: 'data' as const,
    };
    panelStore.usePanelStore.setState({
      panels: [viewPanel],
      activePanelId: viewPanel.id,
    });
    schemaState.activeDbSessionId = 'conn-1';
    seedConnectionSchema(schemaState, 'conn-1', {
      currentDatabase: 'db_b',
      views: [{ name: 'v_users', schema: 'public', tableType: 'view' }],
    });

    render(<ContentView nodeContextMenuRef={nodeContextMenuRef} />);
    expect(nodeContextMenuRef.current).toBeTypeOf('function');

    nodeContextMenuRef.current?.({
      kind: 'view',
      name: 'v_users',
      schema: 'public',
      x: 0,
      y: 0,
    });

    await vi.waitFor(() => {
      expect(executeQueryMock).toHaveBeenCalledWith(
        'conn-1',
        'DROP VIEW "v_users"',
        undefined,
        'db_b',
        'public',
      );
    });
    await vi.waitFor(() => {
      expect(panelStore.usePanelStore.getState().panels).toHaveLength(0);
    });
  });
});
