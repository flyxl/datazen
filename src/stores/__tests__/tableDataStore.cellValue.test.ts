/**
 * The three states of a write, as the store sees them (DB-05).
 *
 * `undefined` (do not write the column), `null` (write SQL `NULL`) and a
 * value (write that value) are three different statements, and the store is
 * where they have to stay apart: the pending change of a row is what the
 * commit plan is built from.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { TableState } from '../tableData/types';

const mockDatabaseCommands = {
  getTableData: vi.fn(),
  previewPendingChanges: vi.fn(),
  commitPendingChanges: vi.fn(),
};

vi.mock('../../lib/session/tableSession', () => ({ tableManualHandle: vi.fn(() => null) }));

vi.mock('../../commands/database', () => ({
  databaseCommands: mockDatabaseCommands,
}));

const sampleColumns = [
  { name: 'id', dataType: 'integer', isPrimaryKey: true, isNullable: false },
  { name: 'name', dataType: 'text', isPrimaryKey: false, isNullable: true },
];

const sampleResponse = {
  columns: sampleColumns,
  rows: [
    [1, 'Alice'],
    [2, 'Bob'],
  ] as (string | number | null)[][],
  totalRows: 2,
  page: 0,
  pageSize: 50,
};

const PANEL = 'panel-users';

describe('tableDataStore: the three states of a write', () => {
  let useTableDataStore: typeof import('../tableDataStore').useTableDataStore;

  const slice = (): TableState => {
    const ts = useTableDataStore.getState().byPanel.get(PANEL);
    if (!ts) throw new Error('panel slice missing');
    return ts;
  };

  const staged = () => [...slice().pendingChanges.values()][0];

  beforeEach(async () => {
    vi.resetModules();
    vi.clearAllMocks();
    mockDatabaseCommands.getTableData.mockResolvedValue(sampleResponse);
    const mod = await import('../tableDataStore');
    useTableDataStore = mod.useTableDataStore;
    useTableDataStore.getState().reset();
    await useTableDataStore.getState().loadTableData({
      panelId: PANEL,
      dbSessionId: 'conn-1',
      table: 'users',
      connectionId: 'conn-1',
      driverType: 'postgres',
      database: 'app',
      schema: null,
    });
  });

  it('stageCellValue with null stores null and lists the column as changed', () => {
    useTableDataStore.getState().stageCellValue(PANEL, 0, 'name', null);
    expect(staged()?.changedColumns).toEqual(['name']);
    expect(staged()?.currentValues.name).toBeNull();
    expect(staged()?.originalValues.name).toBe('Alice');
  });

  it('stageCellValue with a value stores the value and lists the column', () => {
    useTableDataStore.getState().stageCellValue(PANEL, 0, 'name', 'Alicia');
    expect(staged()?.changedColumns).toEqual(['name']);
    expect(staged()?.currentValues.name).toBe('Alicia');
  });

  it('stageCellValue with undefined removes the column change and restores the original row value', () => {
    const store = useTableDataStore.getState();
    store.stageCellValue(PANEL, 0, 'name', 'Alicia');
    expect(slice().rows[0]?.name).toBe('Alicia');

    useTableDataStore.getState().stageCellValue(PANEL, 0, 'name', undefined);
    // The row is clean again: the column left the write entirely.
    expect(slice().pendingChanges.size).toBe(0);
    expect(slice().rows[0]?.name).toBe('Alice');
  });

  it('stageCellValue with undefined on a row without pending changes is a no-op', () => {
    useTableDataStore.getState().stageCellValue(PANEL, 0, 'name', undefined);
    expect(slice().pendingChanges.size).toBe(0);
    expect(slice().rows[0]?.name).toBe('Alice');
  });

  it('offers the tri-state action next to the legacy one', () => {
    // The action exists next to `stageCellChange`, not instead of it.
    expect(typeof useTableDataStore.getState().stageCellValue).toBe('function');
    expect(typeof useTableDataStore.getState().stageCellChange).toBe('function');
  });

  it('stageCellChange keeps today\'s behaviour bit for bit', () => {
    const store = useTableDataStore.getState();
    store.stageCellChange(PANEL, 0, 'name', 'Alicia');
    expect(staged()?.changedColumns).toEqual(['name']);
    expect(staged()?.currentValues.name).toBe('Alicia');

    // An absent value was a NULL before the tri-state existed, and it still
    // is: the legacy entry point cannot carry "do not write".
    useTableDataStore.getState().stageCellChange(PANEL, 0, 'name', undefined);
    expect(staged()?.currentValues.name).toBeNull();
  });

  it('empty string and null produce different pending values', () => {
    const store = useTableDataStore.getState();
    store.stageCellValue(PANEL, 0, 'name', '');
    expect(staged()?.currentValues.name).toBe('');

    useTableDataStore.getState().stageCellValue(PANEL, 0, 'name', null);
    expect(staged()?.currentValues.name).toBeNull();
    expect(staged()?.currentValues.name).not.toBe('');
  });

  it('drops a column that is written back to its original value', () => {
    const store = useTableDataStore.getState();
    store.stageCellValue(PANEL, 0, 'name', 'Alicia');
    useTableDataStore.getState().stageCellValue(PANEL, 0, 'name', 'Alice');
    expect(slice().pendingChanges.size).toBe(0);
  });

  it('refuses a write to a table without a primary key', () => {
    // A row that cannot be addressed is not written; the reason is the one
    // the interface shows.
    mockDatabaseCommands.getTableData.mockResolvedValue({
      columns: [{ name: 'name', dataType: 'text', isPrimaryKey: false, isNullable: true }],
      rows: [['Alice']] as (string | number | null)[][],
      totalRows: 1,
      page: 0,
      pageSize: 50,
    });
    useTableDataStore.getState().reset();
    return useTableDataStore
      .getState()
      .loadTableData({
        panelId: PANEL,
        dbSessionId: 'conn-1',
        table: 'users',
        connectionId: 'conn-1',
        driverType: 'postgres',
        database: 'app',
        schema: null,
      })
      .then(() => {
        useTableDataStore.getState().stageCellValue(PANEL, 0, 'name', 'Alicia');
        expect(slice().pendingChanges.size).toBe(0);
        expect(slice().error).toBeTruthy();
      });
  });
});
