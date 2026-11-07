/**
 * The panel wiring of the dirty state (DB-06 §5 Step 4).
 *
 * The panel owns **one** dirty index: the grid paints the cells it holds and
 * the toolbar shows its counts. This file pins that wiring — and the guard
 * that keeps a row which is marked for deletion out of the editor.
 */
import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { render, fireEvent, cleanup, screen } from '@testing-library/react';
import { TableView } from '../TableView';
import { emptyTableState } from '../../../stores/tableData/connectionState';
import type { TableState } from '../../../stores/tableData/types';

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

vi.mock('../../../lib/databaseTypes', () => ({
  get DB_REGISTRY() {
    return { postgresql: {} } as Record<string, { readOnly?: boolean }>;
  },
  escapeIdent: (ident: string) => ident,
}));

vi.mock('../../../lib/session/tableSession', () => ({
  tableSessionController: vi.fn().mockResolvedValue({
    snapshot: { state: 'ready' },
    refresh: vi.fn().mockResolvedValue({ state: 'ready', observedContext: { transactionState: 'none' } }),
    execute: vi.fn(),
    getExecution: vi.fn(),
  }),
}));

vi.mock('../../../commands/database', () => ({
  databaseCommands: { useDatabase: vi.fn().mockResolvedValue(undefined) },
}));

vi.mock('../../../components/ai/NlFilterInput', () => ({
  NlFilterInput: () => <div data-testid="mock-nl-filter" />,
}));

// The grid is mocked out: the panel's own wiring is what is under test here.
vi.mock('../../../components/DataTable/DataTable', () => ({
  DataTable: (props: {
    onCellDoubleClick?: (row: number, col: string) => void;
    onRestoreRow?: (rowIndex: number) => void;
    dirtyByRow?: ReadonlyMap<number, { state: string }> | null;
  }) => (
    <div data-testid="mock-data-table">
      <div data-testid="mock-dirty-rows">
        {Array.from(props.dirtyByRow?.entries() ?? [])
          .map(([rowIndex, entry]) => `${rowIndex}:${entry.state}`)
          .join(',')}
      </div>
      <button
        type="button"
        data-testid="mock-cell-double-click"
        onClick={() => props.onCellDoubleClick?.(0, 'id')}
      />
      <button
        type="button"
        data-testid="mock-restore-row"
        onClick={() => props.onRestoreRow?.(0)}
      />
    </div>
  ),
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
  setRowModeSelection: vi.fn(),
  toggleSelectAll: vi.fn(),
  setGridSelection: vi.fn(),
  deleteRows: vi.fn(),
  unstageRowDelete: vi.fn(),
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
const COLUMNS = [
  { name: 'id', dataType: 'int', nullable: false, isPrimaryKey: true },
  { name: 'name', dataType: 'varchar', nullable: true, isPrimaryKey: false },
];

const CONTEXT = {
  connectionId: null,
  dbSessionId: 'c1',
  driverType: 'postgresql',
  database: 'app',
  schema: null,
  table: 'users',
};

function seedPanel(overrides: Partial<TableState> = {}) {
  tableStore.byPanel.set(PANEL, {
    ...emptyTableState(CONTEXT),
    columns: COLUMNS,
    rows: [
      { id: 1, name: 'Alice' },
      { id: 2, name: 'Bob' },
    ],
    totalRows: 2,
    ...overrides,
  });
}

function renderTable() {
  return render(
    <TableView
      panelId={PANEL}
      dbSessionId={CONTEXT.dbSessionId}
      database={CONTEXT.database ?? ''}
      tableName={CONTEXT.table}
      connectionId={CONTEXT.connectionId ?? undefined}
      databaseType={CONTEXT.driverType ?? undefined}
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

describe('TableView dirty wiring', () => {
  it('hands the dirty rows of the current page to the grid', () => {
    seedPanel({
      pendingChanges: new Map([
        [
          '["id":1]',
          {
            rowIdentity: { id: 1 },
            originalValues: { name: 'Alice' },
            currentValues: { name: 'Alicia' },
            changedColumns: ['name'],
            deleteMarked: false,
          },
        ],
      ]),
    });

    renderTable();
    expect(screen.getByTestId('mock-dirty-rows')).toHaveTextContent('0:dirty');
    // The toolbar counts the changed cells of the very same index.
    expect(screen.getByTestId('pending-cells-count')).toHaveTextContent('tableData.pendingCells');
  });

  it('hands a delete-marked row to the grid as a deleted row', () => {
    seedPanel({
      pendingChanges: new Map([
        [
          '["id":1]',
          {
            rowIdentity: { id: 1 },
            originalValues: {},
            currentValues: {},
            changedColumns: [],
            deleteMarked: true,
          },
        ],
      ]),
    });

    renderTable();
    expect(screen.getByTestId('mock-dirty-rows')).toHaveTextContent('0:deleted');
  });

  it('shows the number of changed cells in the pending-changes bar', () => {
    seedPanel({
      pendingChanges: new Map([
        [
          '["id":1]',
          {
            rowIdentity: { id: 1 },
            originalValues: { name: 'Alice' },
            currentValues: { name: 'Alicia' },
            changedColumns: ['name'],
            deleteMarked: false,
          },
        ],
        [
          '["id":2]',
          {
            rowIdentity: { id: 2 },
            originalValues: { name: 'Bob' },
            currentValues: { name: 'Bobby' },
            changedColumns: ['name'],
            deleteMarked: false,
          },
        ],
      ]),
    });

    renderTable();
    expect(screen.getByTestId('pending-cells-count')).toBeInTheDocument();
    expect(screen.getByTestId('pending-changes-bar')).toBeInTheDocument();
  });

  it('does not start an edit on a row that is marked for deletion', () => {
    seedPanel({
      pendingChanges: new Map([
        [
          '["id":1]',
          {
            rowIdentity: { id: 1 },
            originalValues: {},
            currentValues: {},
            changedColumns: [],
            deleteMarked: true,
          },
        ],
      ]),
    });

    const { container } = renderTable();
    // The mocked grid reports a double click through a click handler.
    fireEvent.click(
      container.querySelector('[data-testid="mock-cell-double-click"]') as HTMLElement,
    );

    expect(tableStore.startEdit).not.toHaveBeenCalled();
  });

  it('still starts an edit on a row that only has staged cell changes', () => {
    seedPanel({
      pendingChanges: new Map([
        [
          '["id":1]',
          {
            rowIdentity: { id: 1 },
            originalValues: { name: 'Alice' },
            currentValues: { name: 'Alicia' },
            changedColumns: ['name'],
            deleteMarked: false,
          },
        ],
      ]),
    });

    const { container } = renderTable();
    // The mocked grid reports a double click through a click handler.
    fireEvent.click(
      container.querySelector('[data-testid="mock-cell-double-click"]') as HTMLElement,
    );

    expect(tableStore.startEdit).toHaveBeenCalledWith(PANEL, 0, 'id');
  });

  it('the panel wires the restore action to the store', () => {
    seedPanel({
      pendingChanges: new Map([
        [
          '["id":1]',
          {
            rowIdentity: { id: 1 },
            originalValues: {},
            currentValues: {},
            changedColumns: [],
            deleteMarked: true,
          },
        ],
      ]),
    });

    renderTable();
    fireEvent.click(screen.getByTestId('mock-restore-row'));

    expect(tableStore.unstageRowDelete).toHaveBeenCalledWith(PANEL, 0);
  });
});
