import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { render, fireEvent, cleanup, screen } from '@testing-library/react';
import { TableView } from '../TableView';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../hooks/useConfirmDialog', () => ({
  useConfirmDialog: () => [vi.fn().mockResolvedValue(false), null],
}));

vi.mock('../../../stores/settingsStore', () => ({
  useSettingsStore: (sel: (s: { settings: { confirmOnDelete: boolean } }) => unknown) =>
    sel({ settings: { confirmOnDelete: false } }),
}));

vi.mock('../../../commands/database', () => ({
  databaseCommands: {
    useDatabase: vi.fn().mockResolvedValue(undefined),
  },
}));

vi.mock('../../../components/DataTable/DataTable', () => ({
  DataTable: () => <div data-testid="mock-data-table" />,
}));

vi.mock('../../../components/ai/NlFilterInput', () => ({
  NlFilterInput: () => <div data-testid="mock-nl-filter" />,
}));

const tableState = vi.hoisted(() => ({
  tableStates: new Map<string, Record<string, unknown>>(),
  activeTable: 'users',
  loadTableData: vi.fn(),
  switchToTable: vi.fn(),
  setSort: vi.fn(),
  removeFilter: vi.fn(),
  clearFilters: vi.fn(),
  addFilter: vi.fn(),
  updateFilter: vi.fn(),
  setFilterLogic: vi.fn(),
  applyFilters: vi.fn(),
  setFilterPanelOpen: vi.fn(),
  setPage: vi.fn(),
  setPageSize: vi.fn(),
  startEdit: vi.fn(),
  updateCell: vi.fn(),
  cancelEdit: vi.fn(),
  selectRow: vi.fn(),
  toggleSelectAll: vi.fn(),
  deleteRows: vi.fn(),
  setDetailRow: vi.fn(),
  detailRowIndex: null as number | null,
}));

vi.mock('../../../stores/tableDataStore', () => ({
  useTableDataStore: (sel: (s: typeof tableState) => unknown) => sel(tableState),
}));

afterEach(cleanup);

beforeEach(() => {
  vi.clearAllMocks();
  tableState.tableStates = new Map();
  tableState.activeTable = 'users';
  tableState.detailRowIndex = null;
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
    tableState.tableStates.set('users', {
      columns: [],
      rows: [],
      totalRows: 0,
      page: 0,
      pageSize: 50,
      sorts: [],
      filters: [],
      filterLogic: 'and',
      draftFilters: [],
      draftFilterLogic: 'and',
      filterPanelOpen: false,
      editingCell: null,
      selectedRows: new Set<number>(),
      loading: false,
      error: errorMsg,
    });

    render(
      <TableView dbSessionId="c1" database="app" tableName="users" databaseType="postgresql" />,
    );

    const message = screen.getByTestId('copyable-error-message');
    expect(message).toHaveClass('selectable', 'whitespace-pre-wrap', 'break-words');
    expect(message.textContent).toBe(errorMsg);
    expect(screen.queryByText(/truncate/)).toBeNull();
    fireEvent.click(screen.getByTestId('copyable-error-copy'));
    expect(clipboardSpy).toHaveBeenCalledWith(errorMsg);
  });

  it('shows a copyable inline error banner without truncate when reload fails', () => {
    const errorMsg = 'syntax error near filter clause with a very long message that must wrap';
    tableState.tableStates.set('users', {
      columns: [{ name: 'id', dataType: 'int', isPrimaryKey: true }],
      rows: [{ id: 1 }],
      totalRows: 1,
      page: 0,
      pageSize: 50,
      sorts: [],
      filters: [],
      filterLogic: 'and',
      draftFilters: [],
      draftFilterLogic: 'and',
      filterPanelOpen: false,
      editingCell: null,
      selectedRows: new Set<number>(),
      loading: false,
      error: errorMsg,
    });

    render(
      <TableView dbSessionId="c1" database="app" tableName="users" databaseType="postgresql" />,
    );

    const message = screen.getByTestId('copyable-error-message');
    expect(message).toHaveClass('selectable', 'whitespace-pre-wrap', 'break-words');
    expect(message.className).not.toMatch(/truncate/);
    expect(message.textContent).toBe(errorMsg);
    expect(screen.getByTestId('mock-data-table')).toBeInTheDocument();
    fireEvent.click(screen.getByTestId('copyable-error-copy'));
    expect(clipboardSpy).toHaveBeenCalledWith(errorMsg);
  });

  it('loads data through the panel target database on mount (F1 BUG-002)', () => {
    render(
      <TableView dbSessionId="c1" database="db_b" tableName="users" databaseType="postgresql" />,
    );

    expect(tableState.loadTableData).toHaveBeenCalledWith({
      dbSessionId: 'c1',
      table: 'users',
      connectionId: null,
      driverType: 'postgresql',
      database: 'db_b',
      schema: null,
    });
  });

  it('retries failed loads with the panel target database (F1 BUG-002)', () => {
    tableState.tableStates.set('users', {
      columns: [],
      rows: [],
      totalRows: 0,
      page: 0,
      pageSize: 50,
      sorts: [],
      filters: [],
      filterLogic: 'and',
      draftFilters: [],
      draftFilterLogic: 'and',
      filterPanelOpen: false,
      editingCell: null,
      selectedRows: new Set<number>(),
      loading: false,
      error: 'table not found in current database',
    });

    render(
      <TableView dbSessionId="c1" database="db_b" tableName="users" databaseType="postgresql" />,
    );

    fireEvent.click(screen.getByText('common.retry'));
    // The mount effect also fetches once; the retry click must be the last
    // call and must carry the panel's target database.
    expect(tableState.loadTableData).toHaveBeenLastCalledWith({
      dbSessionId: 'c1',
      table: 'users',
      connectionId: null,
      driverType: 'postgresql',
      database: 'db_b',
      schema: null,
    });
  });
});
