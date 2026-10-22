/**
 * The panel-level half of the editor (DB-05).
 *
 * Two things are checked here, and they are the two things a user runs into
 * before any value is typed:
 *
 * - a table **without a primary key** cannot be edited, and the interface
 *   says why (a row that cannot be addressed cannot be written);
 * - a **view** is read-only because it is a view — not because the
 *   connection is.
 *
 * The assertions read the data attributes and the i18n keys, never the
 * rendered wording (12-testing §1.1 D7).
 */
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

vi.mock('../../../stores/settingsStore', () => ({
  useSettingsStore: Object.assign(
    (sel: (s: { settings: typeof settingsState }) => unknown) =>
      sel({ settings: settingsState }),
    { getState: () => ({ settings: settingsState }) },
  ),
}));

vi.mock('../../../stores/connectionStore', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../../stores/connectionStore')>();
  return {
    ...actual,
    useConnectionStore: Object.assign(
      (selector?: (s: unknown) => unknown) => (selector ? selector({ connections: [] }) : {}),
      { getState: () => ({ connections: [] }) },
    ),
  };
});

vi.mock('../../../lib/databaseTypes', () => ({
  get DB_REGISTRY() {
    return { postgresql: {} };
  },
  escapeIdent: (ident: string) => ident,
}));

vi.mock('../../../lib/session/tableSession', () => ({
  tableSessionController: vi.fn().mockResolvedValue({ state: 'ready' }),
}));

vi.mock('../../../commands/database', () => ({
  databaseCommands: { useDatabase: vi.fn().mockResolvedValue(undefined) },
}));

vi.mock('../../../components/DataTable/DataTable', () => ({
  DataTable: (props: {
    columns?: Array<{ id: string; name: string }>;
    onCellDoubleClick?: (row: number, col: string) => void;
    headerActions?: React.ReactNode;
    emptyPlaceholder?: React.ReactNode;
  }) => (
    <div data-testid="mock-data-table">
      {props.headerActions}
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
  stageCellValue: vi.fn(),
  cancelEdit: vi.fn(),
  selectRow: vi.fn(),
  toggleSelectAll: vi.fn(),
  deleteRows: vi.fn(),
  previewPendingChanges: vi.fn().mockResolvedValue(null),
  commitPendingChanges: vi.fn().mockResolvedValue({}),
  rollbackPendingChanges: vi.fn(),
  setDetailRow: vi.fn(),
  setEnumCandidates: vi.fn(),
  clearEnumCandidates: vi.fn(),
}));

vi.mock('../../../stores/tableDataStore', () => ({
  useTableDataStore: Object.assign((sel: (s: typeof tableStore) => unknown) => sel(tableStore), {
    getState: () => tableStore,
  }),
}));

const PANEL = 'panel-users-editors';

const CONTEXT: TableChangeContext = {
  connectionId: null,
  dbSessionId: 'c1',
  driverType: 'postgresql',
  database: 'app',
  schema: null,
  table: 'users',
};

function seedPanel(columns: TableState['columns']): void {
  tableStore.byPanel.set(PANEL, { ...emptyTableState(CONTEXT), columns, rows: [{ id: 1 }] });
}

beforeEach(() => {
  vi.clearAllMocks();
  settingsState.safeMode = false;
  tableStore.byPanel = new Map();
});

afterEach(cleanup);

describe('TableView and the read-only reasons', () => {
  it('double click on a table without a primary key explains the reason and does not start editing', () => {
    seedPanel([
      { name: 'id', dataType: 'integer', nullable: true, isPrimaryKey: false },
      { name: 'name', dataType: 'text', nullable: true, isPrimaryKey: false },
    ]);

    render(
      <TableView
        panelId={PANEL}
        dbSessionId="c1"
        database="app"
        tableName="users"
        databaseType="postgresql"
      />,
    );

    fireEvent.click(screen.getByTestId('mock-cell-double-click'));

    expect(tableStore.startEdit).not.toHaveBeenCalled();
    // The reason is specific: a row that cannot be addressed cannot be
    // written.
    expect(screen.getByTestId('table-read-only-reason')).toHaveTextContent(
      'cellEditor.readOnly.noPrimaryKey',
    );
  });

  it('a view panel reports the view reason, not the connection reason', () => {
    seedPanel([{ name: 'id', dataType: 'integer', nullable: true, isPrimaryKey: true }]);

    render(
      <TableView
        panelId={PANEL}
        dbSessionId="c1"
        database="app"
        tableName="users"
        databaseType="postgresql"
        readOnly={true}
        readOnlyReason="view"
      />,
    );

    fireEvent.click(screen.getByTestId('mock-cell-double-click'));

    expect(tableStore.startEdit).not.toHaveBeenCalled();
    expect(screen.getByTestId('table-read-only-reason')).toHaveTextContent(
      'cellEditor.readOnly.view',
    );
    expect(screen.getByTestId('table-read-only-reason')).not.toHaveTextContent(
      'cellEditor.readOnly.connection',
    );
  });

  it('a read-only connection reports the connection reason', () => {
    seedPanel([{ name: 'id', dataType: 'integer', nullable: true, isPrimaryKey: true }]);

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
    expect(screen.getByTestId('table-read-only-reason')).toHaveTextContent(
      'cellEditor.readOnly.connection',
    );
  });

  it('a table with a primary key opens the editor', () => {
    seedPanel([{ name: 'id', dataType: 'integer', nullable: true, isPrimaryKey: true }]);

    render(
      <TableView
        panelId={PANEL}
        dbSessionId="c1"
        database="app"
        tableName="users"
        databaseType="postgresql"
      />,
    );

    fireEvent.click(screen.getByTestId('mock-cell-double-click'));

    expect(tableStore.startEdit).toHaveBeenCalledWith(PANEL, 0, 'id');
    expect(screen.queryByTestId('table-read-only-tip')).toBeNull();
  });
});
