import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { render, fireEvent, cleanup, screen } from '@testing-library/react';
import { TableView } from '../TableView';
import { emptyTableState } from '../../../stores/tableData/connectionState';
import type { TableState } from '../../../stores/tableData/types';
import type { TableChangeContext } from '../../../lib/tableChanges';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../hooks/useConfirmDialog', () => ({
  useConfirmDialog: () => [vi.fn().mockResolvedValue(false), null],
}));

const settingsState = vi.hoisted(() => ({ confirmOnDelete: false, safeMode: false }));
const connectionsState = vi.hoisted(() => ({
  connections: [] as { id: string; name: string; readOnly?: boolean }[],
}));

vi.mock('../../../stores/settingsStore', () => ({
  useSettingsStore: (sel: (s: { settings: typeof settingsState }) => unknown) =>
    sel({ settings: settingsState }),
}));

vi.mock('../../../stores/connectionStore', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../../stores/connectionStore')>();
  return {
    ...actual,
    useConnectionStore: Object.assign(
      (selector?: (s: typeof connectionsState) => unknown) =>
        selector ? selector(connectionsState) : connectionsState,
      {
        getState: () => connectionsState,
        setState: (partial: Partial<typeof connectionsState>) =>
          Object.assign(connectionsState, partial),
      },
    ),
  };
});

// Controllable registry so we can assert read-only drivers block editing.
const registryState = vi.hoisted(() => ({
  registry: {
    postgresql: {} /* not read-only */,
    superset: { readOnly: true },
  } as Record<string, { readOnly?: boolean }>,
}));

vi.mock('../../../lib/databaseTypes', () => ({
  get DB_REGISTRY() {
    return registryState.registry;
  },
  escapeIdent: (ident: string) => ident,
}));

const queryCommandsMock = vi.hoisted(() => ({
  sessionTransactionStatus: vi.fn().mockResolvedValue(false),
  beginSessionTransaction: vi.fn().mockResolvedValue(undefined),
  commitSessionTransaction: vi.fn().mockResolvedValue(undefined),
  rollbackSessionTransaction: vi.fn().mockResolvedValue(undefined),
}));

vi.mock('../../../commands/query', () => ({
  queryCommands: queryCommandsMock,
}));

vi.mock('../../../commands/database', () => ({
  databaseCommands: {
    useDatabase: vi.fn().mockResolvedValue(undefined),
  },
}));

vi.mock('../../../components/DataTable/DataTable', () => ({
  DataTable: (props: {
    columns?: Array<{ id: string; name: string }>;
    onCellDoubleClick?: (row: number, col: string) => void;
    headerActions?: React.ReactNode;
    emptyPlaceholder?: React.ReactNode;
  }) => (
    <div data-testid="mock-data-table">
      <div data-testid="mock-data-table-columns">{props.columns?.map((c) => c.name).join(',')}</div>
      <div data-testid="mock-data-table-actions">{props.headerActions}</div>
      {props.emptyPlaceholder}
      <button
        type="button"
        data-testid="mock-cell-double-click"
        onClick={() => props.onCellDoubleClick?.(0, 'id')}
      />
    </div>
  ),
}));

vi.mock('../../../components/ai/NlFilterInput', () => ({
  NlFilterInput: () => <div data-testid="mock-nl-filter" />,
}));

const tableStore = vi.hoisted(() => ({
  byPanel: new Map<string, TableState>(),
  loadTableData: vi.fn(),
  reloadPanel: vi.fn(),
  setSort: vi.fn(),
  removeFilter: vi.fn(),
  clearFilters: vi.fn(),
  addFilter: vi.fn(),
  setFilters: vi.fn(),
  updateFilter: vi.fn(),
  setFilterLogic: vi.fn(),
  applyFilters: vi.fn(),
  setFilterPanelOpen: vi.fn(),
  setVisibleColumns: vi.fn(),
  setPage: vi.fn(),
  setPageSize: vi.fn(),
  startEdit: vi.fn(),
  stageCellChange: vi.fn(),
  cancelEdit: vi.fn(),
  selectRow: vi.fn(),
  toggleSelectAll: vi.fn(),
  deleteRows: vi.fn(),
  previewPendingChanges: vi.fn().mockResolvedValue(null),
  commitPendingChanges: vi.fn().mockResolvedValue({}),
  rollbackPendingChanges: vi.fn(),
  setDetailRow: vi.fn(),
}));

vi.mock('../../../stores/tableDataStore', () => ({
  useTableDataStore: Object.assign((sel: (s: typeof tableStore) => unknown) => sel(tableStore), {
    getState: () => tableStore,
  }),
}));

const PANEL = 'panel-users';
const TARGET = {
  dbSessionId: 'c1',
  database: 'app',
  tableName: 'users',
  databaseType: 'postgresql',
};

function contextOf(overrides: Partial<TableChangeContext> = {}): TableChangeContext {
  return {
    connectionId: null,
    dbSessionId: TARGET.dbSessionId,
    driverType: TARGET.databaseType,
    database: TARGET.database,
    schema: null,
    table: TARGET.tableName,
    ...overrides,
  };
}

/** Seed the panel slice with data already loaded, so the mount effect stays quiet. */
function seedPanel(context: TableChangeContext, overrides: Partial<TableState> = {}) {
  tableStore.byPanel.set(PANEL, { ...emptyTableState(context), ...overrides });
}

function renderTable(overrides: Partial<TableChangeContext> = {}) {
  const context = contextOf(overrides);
  return render(
    <TableView
      panelId={PANEL}
      dbSessionId={context.dbSessionId}
      database={context.database ?? ''}
      tableName={context.table}
      connectionId={context.connectionId ?? undefined}
      databaseType={context.driverType ?? undefined}
    />,
  );
}

afterEach(cleanup);

beforeEach(() => {
  vi.clearAllMocks();
  settingsState.confirmOnDelete = false;
  settingsState.safeMode = false;
  connectionsState.connections = [];
  tableStore.byPanel = new Map();
});

describe('TableView', () => {
  let clipboardSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    Object.defineProperty(window.navigator, 'clipboard', {
      configurable: true,
      value: { writeText: vi.fn() },
    });
    clipboardSpy = vi.spyOn(window.navigator.clipboard, 'writeText').mockResolvedValue(undefined);
  });

  it('shows a copyable full-page error when initial load fails', () => {
    const errorMsg = 'permission denied for table users\nDETAIL: role lacks SELECT';
    seedPanel(contextOf(), { error: errorMsg });

    renderTable();

    const message = screen.getByTestId('copyable-error-message');
    expect(message).toHaveClass('selectable', 'whitespace-pre-wrap', 'break-words');
    expect(message.textContent).toBe(errorMsg);
    expect(screen.queryByText(/truncate/)).toBeNull();
    fireEvent.click(screen.getByTestId('copyable-error-copy'));
    expect(clipboardSpy).toHaveBeenCalledWith(errorMsg);
  });

  it('shows a copyable inline error banner without truncate when reload fails', () => {
    const errorMsg = 'syntax error near filter clause with a very long message that must wrap';
    seedPanel(contextOf(), {
      columns: [{ name: 'id', dataType: 'int', nullable: false, isPrimaryKey: true }],
      rows: [{ id: 1 }],
      totalRows: 1,
      error: errorMsg,
    });

    renderTable();

    const message = screen.getByTestId('copyable-error-message');
    expect(message).toHaveClass('selectable', 'whitespace-pre-wrap', 'break-words');
    expect(message.className).not.toMatch(/truncate/);
    expect(message.textContent).toBe(errorMsg);
    expect(screen.getByTestId('mock-data-table')).toBeInTheDocument();
    fireEvent.click(screen.getByTestId('copyable-error-copy'));
    expect(clipboardSpy).toHaveBeenCalledWith(errorMsg);
  });

  it('loads data through the panel target database on mount (F1 BUG-002)', () => {
    renderTable({ database: 'db_b' });

    expect(tableStore.loadTableData).toHaveBeenCalledWith({
      panelId: PANEL,
      dbSessionId: 'c1',
      table: 'users',
      connectionId: null,
      driverType: 'postgresql',
      database: 'db_b',
      schema: null,
    });
  });

  it('retries failed loads with the panel target database (F1 BUG-002)', () => {
    seedPanel(contextOf({ database: 'db_b' }), {
      error: 'table not found in current database',
    });

    renderTable({ database: 'db_b' });

    fireEvent.click(screen.getByText('common.retry'));
    // The mount effect also fetches once; the retry click must be the last
    // call and must carry the panel's target database.
    expect(tableStore.loadTableData).toHaveBeenLastCalledWith({
      panelId: PANEL,
      dbSessionId: 'c1',
      table: 'users',
      connectionId: null,
      driverType: 'postgresql',
      database: 'db_b',
      schema: null,
    });
  });

  it('refresh button re-fetches the current page even when cached data exists', () => {
    seedPanel(contextOf(), {
      columns: [{ name: 'id', dataType: 'int', nullable: false, isPrimaryKey: true }],
      rows: [{ id: 1 }],
      totalRows: 1,
    });

    renderTable();

    // Cached data must not trigger an automatic reload on mount.
    expect(tableStore.loadTableData).not.toHaveBeenCalled();
    fireEvent.click(screen.getByTestId('table-data-refresh'));
    expect(tableStore.reloadPanel).toHaveBeenCalledWith(PANEL);
  });

  it('allows cell editing when safe mode is true', () => {
    settingsState.safeMode = true;

    renderTable();

    fireEvent.click(screen.getByTestId('mock-cell-double-click'));

    expect(tableStore.startEdit).toHaveBeenCalledWith(PANEL, 0, 'id');
    expect(screen.queryByTestId('table-read-only-tip')).toBeNull();
  });

  it('starts a manual transaction from the table toolbar', async () => {
    renderTable();

    await vi.waitFor(() => expect(queryCommandsMock.sessionTransactionStatus).toHaveBeenCalled());
    fireEvent.click(screen.getByTestId('table-tx-begin'));

    await vi.waitFor(() =>
      expect(queryCommandsMock.beginSessionTransaction).toHaveBeenCalledWith('c1'),
    );
  });

  it('blocks cell editing when readOnly prop is true and shows a read-only tip', () => {
    settingsState.safeMode = false;

    render(
      <TableView
        panelId={PANEL}
        dbSessionId="c1"
        database="app"
        tableName="users"
        databaseType="postgresql"
        readOnly={true}
      />,
    );

    fireEvent.click(screen.getByTestId('mock-cell-double-click'));

    expect(tableStore.startEdit).not.toHaveBeenCalled();
    expect(screen.getByTestId('table-read-only-tip')).toHaveTextContent(
      'tableData.readOnlyEditDisabled',
    );
  });

  it('blocks cell editing when saved connection in store is read-only', () => {
    settingsState.safeMode = false;
    connectionsState.connections = [{ id: 'conn-ro', name: 'ReadOnlyConn', readOnly: true }];

    render(
      <TableView
        panelId={PANEL}
        dbSessionId="c1"
        connectionId="conn-ro"
        database="app"
        tableName="users"
        databaseType="postgresql"
      />,
    );

    fireEvent.click(screen.getByTestId('mock-cell-double-click'));

    expect(tableStore.startEdit).not.toHaveBeenCalled();
    expect(screen.getByTestId('table-read-only-tip')).toHaveTextContent(
      'tableData.readOnlyEditDisabled',
    );
  });

  it('blocks cell editing when connection is read-only, and allows when connection is not read-only', () => {
    settingsState.safeMode = false;
    connectionsState.connections = [{ id: 'conn-rw', name: 'WritableConn', readOnly: false }];

    render(
      <TableView
        panelId={PANEL}
        dbSessionId="c1"
        connectionId="conn-rw"
        database="app"
        tableName="users"
        databaseType="postgresql"
      />,
    );

    fireEvent.click(screen.getByTestId('mock-cell-double-click'));

    expect(tableStore.startEdit).toHaveBeenCalledWith(PANEL, 0, 'id');
    expect(screen.queryByTestId('table-read-only-tip')).toBeNull();
  });

  it('blocks cell editing for a read-only driver even when Safe Mode is off', () => {
    settingsState.safeMode = false;
    registryState.registry.superset = { readOnly: true };

    render(
      <TableView
        panelId={PANEL}
        dbSessionId="c1"
        database="app"
        tableName="users"
        databaseType="superset"
      />,
    );

    fireEvent.click(screen.getByTestId('mock-cell-double-click'));
    expect(tableStore.startEdit).not.toHaveBeenCalled();
    expect(screen.getByTestId('table-read-only-tip')).toHaveTextContent(
      'tableData.readOnlyEditDisabled',
    );
  });

  it('renders field filter toggle button in toolbar', () => {
    seedPanel(contextOf(), {
      columns: [
        { name: 'id', dataType: 'int', nullable: false, isPrimaryKey: true },
        { name: 'name', dataType: 'varchar', nullable: true, isPrimaryKey: false },
      ],
      rows: [{ id: 1, name: 'Alice' }],
      totalRows: 1,
    });

    renderTable();

    expect(screen.getByTestId('table-column-filter-toggle')).toBeInTheDocument();
    expect(screen.getByTestId('mock-data-table-columns')).toHaveTextContent('id,name');
  });

  it('filters displayed columns to only user-selected columns', () => {
    seedPanel(contextOf(), {
      columns: [
        { name: 'id', dataType: 'int', nullable: false, isPrimaryKey: true },
        { name: 'name', dataType: 'varchar', nullable: true, isPrimaryKey: false },
        { name: 'email', dataType: 'varchar', nullable: true, isPrimaryKey: false },
      ],
      rows: [{ id: 1, name: 'Alice', email: 'alice@example.com' }],
      totalRows: 1,
      visibleColumns: ['name', 'email'],
    });

    renderTable();

    expect(screen.getByTestId('mock-data-table-columns')).toHaveTextContent('name,email');
  });

  it('shows all-columns-hidden empty state when user deselects all columns', () => {
    seedPanel(contextOf(), {
      columns: [
        { name: 'id', dataType: 'int', nullable: false, isPrimaryKey: true },
        { name: 'name', dataType: 'varchar', nullable: true, isPrimaryKey: false },
      ],
      rows: [{ id: 1, name: 'Alice' }],
      totalRows: 1,
      visibleColumns: [],
    });

    renderTable();

    expect(screen.getByTestId('all-columns-hidden-state')).toBeInTheDocument();
    expect(screen.getByText('tableData.allColumnsHidden')).toBeInTheDocument();

    fireEvent.click(screen.getByText('tableData.resetColumns'));
    expect(tableStore.setVisibleColumns).toHaveBeenCalledWith(PANEL, null);
  });

  it('does not trigger React hook count mismatch when transitioning from loading to loaded', () => {
    seedPanel(contextOf(), { loading: true });

    const { rerender } = renderTable();
    expect(screen.getByText('tableView.loadingData')).toBeInTheDocument();

    seedPanel(contextOf(), {
      columns: [{ name: 'id', dataType: 'int', nullable: false, isPrimaryKey: true }],
      rows: [{ id: 1 }],
      totalRows: 1,
    });

    expect(() => {
      rerender(
        <TableView
          panelId={PANEL}
          dbSessionId="c1"
          database="app"
          tableName="users"
          databaseType="postgresql"
        />,
      );
    }).not.toThrow();

    expect(screen.getByTestId('mock-data-table')).toBeInTheDocument();
  });
});
