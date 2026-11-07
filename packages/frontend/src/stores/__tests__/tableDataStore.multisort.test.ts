/**
 * The multi-column sort half of the table-data store (DB-11).
 *
 * The PRD acceptance this file locks, case by case:
 *
 * - 「加第二列不丢第一列」 — `addSort` appends; TablePlus's #2843 was adding a
 *   column and silently losing the previous one.
 * - 「重开应用顺序一致」 — the sort list persists per table identity and is
 *   the seed of a brand-new panel.
 * - positions are part of the ask: the list order is the `ORDER BY` order.
 *
 * Like the keyset sibling, these cases drive the **real store actions** and
 * only the IPC command is a double; the assertions are on what the request
 * carries and what the panel remembers.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { SortCondition } from '../../types';

const mockDatabaseCommands = {
  getTableData: vi.fn(),
  previewPendingChanges: vi.fn(),
  commitPendingChanges: vi.fn(),
};

vi.mock('../../lib/session/tableSession', () => ({ tableManualHandle: vi.fn(() => null) }));

vi.mock('../../commands/database', () => ({
  databaseCommands: mockDatabaseCommands,
}));

const PANEL = 'panel-users';

const sampleColumns = [
  { name: 'id', dataType: 'integer', isPrimaryKey: true, isNullable: false },
  { name: 'name', dataType: 'text', isPrimaryKey: false, isNullable: true },
  { name: 'age', dataType: 'integer', isPrimaryKey: false, isNullable: true },
];

const sampleResponse = {
  columns: sampleColumns,
  rows: [
    [1, 'Alice', 30],
    [2, 'Bob', 40],
  ] as (string | number | null)[][],
  totalRows: 2,
  page: 0,
  pageSize: 50,
};

describe('the multi-column sort half of the table-data store (DB-11)', () => {
  let useTableDataStore: typeof import('../tableDataStore').useTableDataStore;

  const loaded = () => {
    const ts = useTableDataStore.getState().byPanel.get(PANEL);
    if (!ts) throw new Error('the panel has no table-data slice');
    return ts;
  };

  /** The `sorts` of the last request the store sent. */
  function lastRequestSorts(): SortCondition[] | undefined {
    const calls = mockDatabaseCommands.getTableData.mock.calls;
    const last = calls[calls.length - 1];
    if (!last) throw new Error('no request was sent');
    return last[0].sorts as SortCondition[] | undefined;
  }

  /** The storage key of the table this suite loads. */
  function storedKey(): string {
    return 'datazen:table-sort:v1:' + JSON.stringify(['conn-1', 'app', '', 'users']);
  }

  beforeEach(async () => {
    vi.resetModules();
    vi.clearAllMocks();
    window.localStorage.clear();
    mockDatabaseCommands.getTableData.mockResolvedValue(sampleResponse);
    const mod = await import('../tableDataStore');
    useTableDataStore = mod.useTableDataStore;
    useTableDataStore.getState().reset();
  });

  async function loadTable() {
    await useTableDataStore.getState().loadTableData({
      panelId: PANEL,
      dbSessionId: 'conn-1',
      table: 'users',
      connectionId: 'conn-1',
      driverType: 'postgres',
      database: 'app',
      schema: null,
    });
  }

  it('addSort keeps the first column and appends the second', async () => {
    await loadTable();
    useTableDataStore.getState().addSort(PANEL, { column: 'name', descending: false });
    await vi.waitFor(() => expect(loaded().sorts).toEqual([{ column: 'name', descending: false }]));

    mockDatabaseCommands.getTableData.mockClear();
    useTableDataStore.getState().addSort(PANEL, { column: 'age', descending: true });
    await vi.waitFor(() => expect(mockDatabaseCommands.getTableData).toHaveBeenCalled());
    // The acceptance, one line: the first column survived the second one.
    expect(loaded().sorts).toEqual([
      { column: 'name', descending: false },
      { column: 'age', descending: true },
    ]);
    // And the read asked for both, in list order.
    expect(lastRequestSorts()).toEqual(loaded().sorts);
  });

  it('addSort on an already-sorted column changes its direction in place', async () => {
    await loadTable();
    useTableDataStore.getState().addSort(PANEL, { column: 'name', descending: false });
    useTableDataStore.getState().addSort(PANEL, { column: 'age', descending: false });
    await vi.waitFor(() => expect(loaded().sorts).toHaveLength(2));

    useTableDataStore.getState().addSort(PANEL, { column: 'name', descending: true });
    await vi.waitFor(() =>
      expect(lastRequestSorts()).toEqual([
        { column: 'name', descending: true },
        { column: 'age', descending: false },
      ]),
    );
    // In place: still two columns, `name` still first.
    expect(loaded().sorts).toEqual([
      { column: 'name', descending: true },
      { column: 'age', descending: false },
    ]);
  });

  it('removeSort drops one column and keeps the others', async () => {
    await loadTable();
    useTableDataStore.getState().addSort(PANEL, { column: 'name', descending: false });
    useTableDataStore.getState().addSort(PANEL, { column: 'age', descending: true });
    await vi.waitFor(() => expect(loaded().sorts).toHaveLength(2));

    mockDatabaseCommands.getTableData.mockClear();
    useTableDataStore.getState().removeSort(PANEL, 'name');
    await vi.waitFor(() => expect(mockDatabaseCommands.getTableData).toHaveBeenCalled());
    expect(loaded().sorts).toEqual([{ column: 'age', descending: true }]);
  });

  it('clearSorts empties the list', async () => {
    await loadTable();
    useTableDataStore.getState().addSort(PANEL, { column: 'name', descending: false });
    await vi.waitFor(() => expect(loaded().sorts).toHaveLength(1));

    mockDatabaseCommands.getTableData.mockClear();
    useTableDataStore.getState().clearSorts(PANEL);
    await vi.waitFor(() => expect(mockDatabaseCommands.getTableData).toHaveBeenCalled());
    expect(loaded().sorts).toEqual([]);
  });

  it('a sort action resets the page to the first one', async () => {
    await loadTable();
    useTableDataStore.getState().setPage(PANEL, 3);
    await vi.waitFor(() => expect(loaded().page).toBe(3));

    useTableDataStore.getState().addSort(PANEL, { column: 'name', descending: false });
    await vi.waitFor(() => expect(loaded().page).toBe(0));
  });

  it('persists the sort list and seeds a brand-new panel with it', async () => {
    await loadTable();
    useTableDataStore.getState().addSort(PANEL, { column: 'name', descending: false });
    useTableDataStore.getState().addSort(PANEL, { column: 'age', descending: true });
    await vi.waitFor(() => expect(loaded().sorts).toHaveLength(2));

    // The list is in storage, keyed by the table's identity.
    expect(JSON.parse(window.localStorage.getItem(storedKey())!)).toEqual([
      { column: 'name', descending: false },
      { column: 'age', descending: true },
    ]);

    // Close the tab, reopen it: the panel starts from the stored list —
    // 「重开应用顺序一致」.
    useTableDataStore.getState().removePanel(PANEL);
    expect(useTableDataStore.getState().byPanel.get(PANEL)).toBeUndefined();

    mockDatabaseCommands.getTableData.mockClear();
    await loadTable();
    await vi.waitFor(() => expect(mockDatabaseCommands.getTableData).toHaveBeenCalled());
    expect(loaded().sorts).toEqual([
      { column: 'name', descending: false },
      { column: 'age', descending: true },
    ]);
    expect(lastRequestSorts()).toEqual([
      { column: 'name', descending: false },
      { column: 'age', descending: true },
    ]);
  });

  it('clearing the sort list clears the stored entry too', async () => {
    await loadTable();
    useTableDataStore.getState().addSort(PANEL, { column: 'name', descending: false });
    await vi.waitFor(() => expect(loaded().sorts).toHaveLength(1));
    expect(window.localStorage.getItem(storedKey())).toBeTruthy();

    useTableDataStore.getState().clearSorts(PANEL);
    await vi.waitFor(() => expect(loaded().sorts).toEqual([]));
    expect(window.localStorage.getItem(storedKey())).toBeNull();
  });

  it('a panel whose table has no configured connection does not persist', async () => {
    await useTableDataStore.getState().loadTableData({
      panelId: PANEL,
      dbSessionId: 'session-only',
      table: 'users',
      connectionId: null,
      driverType: 'postgres',
      database: 'app',
      schema: null,
    });
    useTableDataStore.getState().addSort(PANEL, { column: 'name', descending: false });
    await vi.waitFor(() => expect(loaded().sorts).toHaveLength(1));

    const keys = Object.keys(window.localStorage).filter((key) =>
      key.startsWith('datazen:table-sort:v1:'),
    );
    expect(keys).toEqual([]);
  });

  it('corrupt or shapeless storage reads as no stored sorts', async () => {
    window.localStorage.setItem(storedKey(), 'not json at all');
    await loadTable();
    expect(loaded().sorts).toEqual([]);

    window.localStorage.setItem(storedKey(), JSON.stringify([{ column: 7, descending: 'yes' }]));
    useTableDataStore.getState().removePanel(PANEL);
    await loadTable();
    expect(loaded().sorts).toEqual([]);
  });

  it('the stored list of another table never leaks into this one', async () => {
    window.localStorage.setItem(
      'datazen:table-sort:v1:' + JSON.stringify(['conn-1', 'app', '', 'orders']),
      JSON.stringify([{ column: 'order_date', descending: true }]),
    );
    await loadTable();
    expect(loaded().sorts).toEqual([]);
  });
});
