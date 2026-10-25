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

const settingsState = vi.hoisted(() => ({
  confirmOnDelete: false,
  safeMode: false,
  rowCountMode: 'auto' as const,
  rowCountEstimateThreshold: 200000,
  defaultTableSort: 'primaryKeyAsc' as const,
}));
const connectionsState = vi.hoisted(() => ({
  connections: [] as { id: string; name: string; readOnly?: boolean }[],
}));

vi.mock('../../../stores/settingsStore', () => ({
  // The store is read both as a hook and as a plain snapshot (the read path
  // asks for the count strategy outside of React), so the mock carries both.
  useSettingsStore: Object.assign(
    (sel: (s: { settings: typeof settingsState }) => unknown) => sel({ settings: settingsState }),
    { getState: () => ({ settings: settingsState }) },
  ),
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

const tableController = vi.hoisted(() => ({
  snapshot: { state: 'ready' },
  refresh: vi
    .fn()
    .mockResolvedValue({ state: 'ready', observedContext: { transactionState: 'none' } }),
  execute: vi.fn().mockResolvedValue({ executionId: 'table-tx' }),
  getExecution: vi.fn().mockResolvedValue({ state: 'succeeded' }),
}));
vi.mock('../../../lib/session/tableSession', () => ({
  tableSessionController: vi.fn().mockResolvedValue(tableController),
  // DB-15: the preview request carries the panel's manual handle; none here.
  tableManualHandle: () => null,
}));

// DB-15: the SQL preview goes through the real command — the mock only pins
// its answer, so the request the test asserts is the request the panel sends.
const databaseCommandsMock = vi.hoisted(() => ({
  useDatabase: vi.fn().mockResolvedValue(undefined),
  previewTableReadSql: vi.fn(),
}));

vi.mock('../../../commands/database', () => ({
  databaseCommands: databaseCommandsMock,
}));

vi.mock('../../../components/DataTable/DataTable', () => ({
  DataTable: (props: {
    columns?: Array<{ id: string; name: string }>;
    onCellDoubleClick?: (row: number, col: string) => void;
    headerActions?: React.ReactNode;
    emptyPlaceholder?: React.ReactNode;
    onCellEdit?: unknown;
    enableSetNull?: boolean;
    onDeleteRows?: unknown;
    onPreviewSql?: () => void;
    onExportFiltered?: () => void;
    exportOpenSignal?: number;
  }) => (
    <div data-testid="mock-data-table">
      <div data-testid="mock-data-table-columns">{props.columns?.map((c) => c.name).join(',')}</div>
      <div data-testid="mock-data-table-actions">{props.headerActions}</div>
      {/* DB-16 entrance gates, exposed as attributes so the assertion can be
          string-level: "was the callback withdrawn at the entrance?" */}
      <div
        data-testid="mock-data-table-gates"
        data-cell-edit={props.onCellEdit != null ? 'on' : 'off'}
        data-set-null={props.enableSetNull ? 'on' : 'off'}
        data-delete-rows={props.onDeleteRows != null ? 'on' : 'off'}
      />
      {/* DB-15: the filter panel's exits, as clickable probes plus the signal. */}
      <div
        data-testid="mock-data-table-db15"
        data-export-signal={String(props.exportOpenSignal ?? 0)}
      />
      {props.onPreviewSql && (
        <button type="button" data-testid="mock-preview-sql-click" onClick={props.onPreviewSql} />
      )}
      {props.onExportFiltered && (
        <button
          type="button"
          data-testid="mock-export-filtered-click"
          onClick={props.onExportFiltered}
        />
      )}
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

vi.mock('../../../stores/tableDataStore', async (importOriginal) => {
  // DB-15: `orderRequest` is a real pure function the preview mirrors — keep
  // the real one, replace only the reactive store.
  const actual = await importOriginal<typeof import('../../../stores/tableDataStore')>();
  return {
    ...actual,
    useTableDataStore: Object.assign((sel: (s: typeof tableStore) => unknown) => sel(tableStore), {
      getState: () => tableStore,
    }),
  };
});

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

  it('loads data through the panel target database on mount', () => {
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

  it('retries failed loads with the panel target database', () => {
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

    await vi.waitFor(() => expect(tableController.refresh).toHaveBeenCalled());
    fireEvent.click(screen.getByTestId('table-tx-begin'));

    await vi.waitFor(() =>
      expect(tableController.execute).toHaveBeenCalledWith({
        command: 'platform.beginTransaction',
        input: { defersCommit: true },
      }),
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

describe('TableView no-primary-key escape hatch (DB-16)', () => {
  /** Seed a loaded, PK-less page — the state every DB-16 case starts from. */
  function seedNoPkPanel() {
    seedPanel(contextOf(), {
      columns: [{ name: 'token', dataType: 'text', nullable: true, isPrimaryKey: false }],
      rows: [{ token: 'a1' }],
      totalRows: 1,
    });
  }

  it('keeps the pre-DB-16 banner byte-for-byte when no handler is threaded', () => {
    seedNoPkPanel();

    renderTable();

    // The order footnote only — no banner button, no editability claim.
    expect(screen.getByText('defaultSort.noPrimaryKey')).toBeInTheDocument();
    expect(screen.queryByTestId('table-no-primary-key-edit-in-sql')).toBeNull();
    expect(screen.queryByText('tableNoPk.editBanner')).toBeNull();
  });

  it('swaps the footnote for the editability banner with a working action', () => {
    seedNoPkPanel();
    const onEditInSql = vi.fn();

    render(
      <TableView
        panelId={PANEL}
        dbSessionId="c1"
        database="app"
        tableName="users"
        databaseType="postgresql"
        onEditInSql={onEditInSql}
      />,
    );

    expect(screen.getByText('tableNoPk.editBanner')).toBeInTheDocument();
    expect(screen.queryByText('defaultSort.noPrimaryKey')).toBeNull();

    fireEvent.click(screen.getByTestId('table-no-primary-key-edit-in-sql'));
    expect(onEditInSql).toHaveBeenCalledTimes(1);
  });

  it('keeps the editability banner even when the user has chosen a custom order', () => {
    seedNoPkPanel();
    // A user pick (or a named view) switches the effective order to 'none' —
    // the old footnote would disappear, but "cannot edit here" stays true.
    tableStore.byPanel.set(PANEL, {
      ...emptyTableState(contextOf()),
      columns: [{ name: 'token', dataType: 'text', nullable: true, isPrimaryKey: false }],
      rows: [{ token: 'a1' }],
      totalRows: 1,
      sorts: [{ column: 'token', descending: true }],
    });

    render(
      <TableView
        panelId={PANEL}
        dbSessionId="c1"
        database="app"
        tableName="users"
        databaseType="postgresql"
        onEditInSql={vi.fn()}
      />,
    );

    expect(screen.getByText('tableNoPk.editBanner')).toBeInTheDocument();
  });

  it('falls back to the footnote banner on a read-only connection', () => {
    seedNoPkPanel();

    render(
      <TableView
        panelId={PANEL}
        dbSessionId="c1"
        database="app"
        tableName="users"
        databaseType="postgresql"
        readOnly={true}
        onEditInSql={vi.fn()}
      />,
    );

    // Read-only is a connection-wide fact with its own tip; the no-PK banner
    // must not promise an escape hatch the connection refuses to honor.
    expect(screen.getByText('defaultSort.noPrimaryKey')).toBeInTheDocument();
    expect(screen.queryByTestId('table-no-primary-key-edit-in-sql')).toBeNull();
  });

  it('withdraws the in-place write entrances on a writable keyless table (no click-then-fail paths)', () => {
    seedNoPkPanel();

    renderTable();

    // DB-16 entrance hardening: cell edits, "Set NULL" and row deletes are
    // withdrawn even without the SQL exit threaded — the data cannot be
    // addressed, so no entrance may look usable and fail at stage time.
    const gates = screen.getByTestId('mock-data-table-gates');
    expect(gates).toHaveAttribute('data-cell-edit', 'off');
    expect(gates).toHaveAttribute('data-set-null', 'off');
    expect(gates).toHaveAttribute('data-delete-rows', 'off');
  });

  it('keeps the write entrances live on a writable table that has a primary key', () => {
    seedPanel(contextOf(), {
      columns: [{ name: 'id', dataType: 'int', nullable: false, isPrimaryKey: true }],
      rows: [{ id: 1 }],
      totalRows: 1,
    });

    renderTable();

    const gates = screen.getByTestId('mock-data-table-gates');
    expect(gates).toHaveAttribute('data-cell-edit', 'on');
    expect(gates).toHaveAttribute('data-set-null', 'on');
    expect(gates).toHaveAttribute('data-delete-rows', 'on');
  });

  it('keeps the write entrances withdrawn on a read-only connection with a primary key', () => {
    seedPanel(contextOf(), {
      columns: [{ name: 'id', dataType: 'int', nullable: false, isPrimaryKey: true }],
      rows: [{ id: 1 }],
      totalRows: 1,
    });

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

    // The pre-existing read-only wiring, re-pinned so the keyless hardening
    // cannot regress the plain read-only path.
    const gates = screen.getByTestId('mock-data-table-gates');
    expect(gates).toHaveAttribute('data-cell-edit', 'off');
    expect(gates).toHaveAttribute('data-set-null', 'off');
    expect(gates).toHaveAttribute('data-delete-rows', 'off');
  });
});

// DB-15: the SQL preview and the filtered export of the filter panel.
describe('TableView SQL preview (DB-15)', () => {
  it('previews the statements the next read would send, with the read\u2019s own parameters', async () => {
    seedPanel(contextOf(), {
      columns: [
        { name: 'id', dataType: 'int', nullable: false, isPrimaryKey: true },
        { name: 'status', dataType: 'varchar', nullable: true, isPrimaryKey: false },
      ],
      rows: [{ id: 1, status: 'active' }],
      totalRows: 1,
      page: 2,
      pageSize: 50,
      filters: [{ column: 'status', operator: 'eq', value: 'active' }],
      sorts: [{ column: 'id', descending: true }],
    });
    databaseCommandsMock.previewTableReadSql.mockResolvedValue({
      dataSql:
        'SELECT "id", "status" FROM "users" WHERE "status" = \'active\' ORDER BY "id" DESC LIMIT 50 OFFSET 100;',
      countSql: 'SELECT COUNT(*) FROM "users" WHERE "status" = \'active\';',
      exactCountSql: 'SELECT COUNT(*) FROM "users" WHERE "status" = \'active\';',
      countKind: 'exact',
    });

    renderTable();

    fireEvent.click(screen.getByTestId('mock-preview-sql-click'));

    await vi.waitFor(() => expect(databaseCommandsMock.previewTableReadSql).toHaveBeenCalledOnce());
    // The request mirrors what `loadTableData` sends: same filters, same
    // sorts, same page — the shown statement is the executed one.
    const request = databaseCommandsMock.previewTableReadSql.mock.calls[0][0] as Record<
      string,
      unknown
    >;
    expect(request.page).toBe(2);
    expect(request.pageSize).toBe(50);
    expect(request.filters).toEqual([{ column: 'status', operator: 'eq', value: 'active' }]);
    expect(request.sorts).toEqual([{ column: 'id', descending: true }]);
    expect(request.unbounded).toBeUndefined();

    // The dialog appears with the statements once the (micro-task) answer lands.
    await vi.waitFor(() =>
      expect(screen.getByTestId('filter-sql-preview-data')).toBeInTheDocument(),
    );
    expect(screen.getByTestId('filter-sql-preview-data').textContent).toContain(
      'ORDER BY "id" DESC LIMIT 50 OFFSET 100',
    );
    expect(screen.getByTestId('filter-sql-preview-count').textContent).toContain(
      'SELECT COUNT(*) FROM "users"',
    );
  });

  it('shows the preview error instead of statements when the backend refuses', async () => {
    seedPanel(contextOf(), {
      columns: [{ name: 'id', dataType: 'int', nullable: false, isPrimaryKey: true }],
      rows: [{ id: 1 }],
      totalRows: 1,
    });
    databaseCommandsMock.previewTableReadSql.mockRejectedValue(new Error('operator not supported'));

    renderTable();

    fireEvent.click(screen.getByTestId('mock-preview-sql-click'));

    await vi.waitFor(() =>
      expect(screen.getByTestId('filter-sql-preview-error')).toHaveTextContent(
        'operator not supported',
      ),
    );
    expect(screen.queryByTestId('filter-sql-preview-data')).toBeNull();
  });

  it('bumps the export signal when the filter panel\u2019s export exit is used', () => {
    seedPanel(contextOf(), {
      columns: [{ name: 'id', dataType: 'int', nullable: false, isPrimaryKey: true }],
      rows: [{ id: 1 }],
      totalRows: 1,
    });

    renderTable();

    const probe = screen.getByTestId('mock-data-table-db15');
    expect(probe).toHaveAttribute('data-export-signal', '0');
    fireEvent.click(screen.getByTestId('mock-export-filtered-click'));
    expect(probe).toHaveAttribute('data-export-signal', '1');
  });
});
