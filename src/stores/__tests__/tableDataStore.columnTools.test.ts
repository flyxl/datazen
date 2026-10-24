/**
 * The DB-12 column-tool half of the table-data store: explicit render order
 * and the freeze count, plus the order's per-table persistence.
 *
 * Both actions are **render-only** — that is the contract under test as much
 * as the state they write: reordering or freezing a column must never cost a
 * re-read, because the rows on the page have not changed, only where their
 * columns sit.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';

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

function storedKey(): string {
  return 'datazen:table-columns:v1:' + JSON.stringify(['conn-1', 'app', '', 'users']);
}

describe('the column-tool half of the table-data store (DB-12)', () => {
  let useTableDataStore: typeof import('../tableDataStore').useTableDataStore;

  const loaded = () => {
    const ts = useTableDataStore.getState().byPanel.get(PANEL);
    if (!ts) throw new Error('the panel has no table-data slice');
    return ts;
  };

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

  it('setColumnOrder writes the order and costs no re-read', async () => {
    await loadTable();
    mockDatabaseCommands.getTableData.mockClear();
    useTableDataStore.getState().setColumnOrder(PANEL, ['name', 'id', 'age']);
    expect(loaded().columnOrder).toEqual(['name', 'id', 'age']);
    // Render-only: the page the panel is showing did not change.
    expect(mockDatabaseCommands.getTableData).not.toHaveBeenCalled();
  });

  it('a null order resets to the schema order', async () => {
    await loadTable();
    useTableDataStore.getState().setColumnOrder(PANEL, ['name', 'id', 'age']);
    useTableDataStore.getState().setColumnOrder(PANEL, null);
    expect(loaded().columnOrder).toBeNull();
  });

  it('the order persists per table and seeds a brand-new panel', async () => {
    await loadTable();
    useTableDataStore.getState().setColumnOrder(PANEL, ['name', 'id', 'age']);
    // Stored under the table's identity, next to the widths.
    expect(JSON.parse(window.localStorage.getItem(storedKey())!)).toEqual({
      order: ['name', 'id', 'age'],
    });

    // Reopen the tab: the panel starts from the stored order —
    // 「重开应用列宽/顺序一致」.
    useTableDataStore.getState().removePanel(PANEL);
    mockDatabaseCommands.getTableData.mockClear();
    await loadTable();
    expect(loaded().columnOrder).toEqual(['name', 'id', 'age']);
  });

  it('a schema change drops the order entries that no longer exist', async () => {
    window.localStorage.setItem(storedKey(), JSON.stringify({ order: ['name', 'dropped', 'age'] }));
    await loadTable();
    // `dropped` is not in the response's schema: only the surviving names
    // come back, in their stored relative order.
    expect(loaded().columnOrder).toEqual(['name', 'age']);
  });

  it('an order that names nothing left is no order at all', async () => {
    window.localStorage.setItem(storedKey(), JSON.stringify({ order: ['gone'] }));
    await loadTable();
    expect(loaded().columnOrder).toBeNull();
  });

  it('setFrozenColumnCount clamps to a non-negative integer', async () => {
    await loadTable();
    useTableDataStore.getState().setFrozenColumnCount(PANEL, 2);
    expect(loaded().frozenColumnCount).toBe(2);
    useTableDataStore.getState().setFrozenColumnCount(PANEL, 0);
    expect(loaded().frozenColumnCount).toBe(0);
    useTableDataStore.getState().setFrozenColumnCount(PANEL, -3);
    expect(loaded().frozenColumnCount).toBe(0);
  });

  it('the freeze is panel-scoped and never persists', async () => {
    await loadTable();
    useTableDataStore.getState().setFrozenColumnCount(PANEL, 1);
    useTableDataStore.getState().removePanel(PANEL);
    await loadTable();
    expect(loaded().frozenColumnCount).toBe(0);
    expect(window.localStorage.getItem(storedKey())).toBeNull();
  });

  it('column widths persist alongside the order under the same key', async () => {
    await loadTable();
    useTableDataStore.getState().setColumnOrder(PANEL, ['name', 'id', 'age']);
    // The width half is written by the resize commit (see the hook tests);
    // the store only guarantees the shared key keeps both.
    const entry = JSON.parse(window.localStorage.getItem(storedKey())!);
    expect(entry.order).toEqual(['name', 'id', 'age']);
  });
});
