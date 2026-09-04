import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, screen } from '@testing-library/react';
import type { ReactNode } from 'react';

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
    getConnectionViewMock: vi.fn(() => MockRedisView),
    MockRedisView,
    schemaState: {
      activeDbSessionId: null as string | null,
      currentDatabase: null as string | null,
      tables: [] as { name: string; schema?: string }[],
      views: [] as { name: string; schema?: string }[],
      databases: [] as string[],
      schemas: new Map<string, { currentDatabase?: string | null }>(),
      loadForConnection: vi.fn(),
      loadTables: vi.fn(),
      removeRelation: vi.fn(),
    },
    tableDataState: {
      columns: [] as { name: string; dataType: string }[],
      rows: [] as Record<string, unknown>[],
      totalRows: 0,
      selectedRows: new Set<number>(),
      tableName: null as string | null,
      detailRowIndex: null as number | null,
      setDatabaseType: vi.fn(),
      updateCell: vi.fn(),
      applyColumnToRows: vi.fn(),
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
  vi.fn((items: Array<{ id?: string; action?: () => void }>) => {
    items.find((item) => item.id === 'drop')?.action?.();
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

vi.mock('../../../stores/schemaStore', () => ({
  useSchemaStore: Object.assign((sel: (s: typeof schemaState) => unknown) => sel(schemaState), {
    getState: () => schemaState,
  }),
}));

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

vi.mock('../schema-tree/SchemaTree', () => ({
  SchemaTree: mockDiv('mock-schema-tree'),
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
    schemaState.activeDbSessionId = null;
    schemaState.currentDatabase = null;
    schemaState.tables = [];
    schemaState.views = [];
    schemaState.schemas = new Map();
    schemaState.loadForConnection = vi.fn();
    schemaState.loadTables = vi.fn();
    schemaState.removeRelation = vi.fn();
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
      subTab: 'data' as const,
    };
    panelStore.usePanelStore.setState({
      panels: [panel],
      activePanelId: panel.id,
    });

    render(<ContentView />);
    expect(screen.getByText(/users/)).toBeInTheDocument();
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
      subTab: 'data' as const,
    };
    panelStore.usePanelStore.setState({
      panels: [panel],
      activePanelId: panel.id,
    });
    schemaState.activeDbSessionId = 'conn-1';
    schemaState.currentDatabase = 'db_b';
    schemaState.tables = [{ name: 'users', schema: 'public' }];
    schemaState.loadForConnection = vi.fn();

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
});
