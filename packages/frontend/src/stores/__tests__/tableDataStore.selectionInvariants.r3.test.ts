/**
 * DB-01 round-3 edge coverage for the GS2-D2 fix and the new §4 invariant boundary
 * ("mode is the row set's single authority; `row` + empty row set is legal").
 *
 * The main suite already covers one cell-mode delete. This file pins:
 * - multi-row delete in cell mode: geometry and row set stay untouched, delete scope still
 *   reaches `pendingChanges` (it comes from the explicit indices, not from `selectedRows`);
 * - an empty index list is a true no-op;
 * - row mode survives a delete — including deleting the selected row itself;
 * - "row mode, zero rows" is a legal resting state (contract 落地校正) and the next
 *   `setRowModeSelection` still works after it;
 * - the invariant matrix for every public writer: `none`/`cell` never carry rows,
 *   `row` may carry any row set.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { TableState } from '../tableData/types';
import {
  EMPTY_GRID_SELECTION,
  selectionFromAnchorFocus,
} from '../tableData/gridSelection';

const mockDatabaseCommands = {
  getTableData: vi.fn(),
  previewPendingChanges: vi.fn(),
  commitPendingChanges: vi.fn(),
};

vi.mock('../../lib/session/tableSession', () => ({ tableManualHandle: vi.fn(() => null) }));
vi.mock('../../commands/database', () => ({ databaseCommands: mockDatabaseCommands }));

const PANEL = 'panel-selection-invariants-r3';

const response = {
  columns: [
    { name: 'id', dataType: 'integer', isPrimaryKey: true, isNullable: false },
    { name: 'name', dataType: 'text', isPrimaryKey: false, isNullable: true },
  ],
  rows: [
    [1, 'Alice'],
    [2, 'Bob'],
  ] as (string | number | null)[][],
  totalRows: 2,
  page: 0,
  pageSize: 50,
};

describe('row-set authority around deleteRows (round 3)', () => {
  let useTableDataStore: typeof import('../tableDataStore').useTableDataStore;

  const loaded = (): TableState => {
    const ts = useTableDataStore.getState().byPanel.get(PANEL);
    if (!ts) throw new Error(`panel ${PANEL} has no table-data slice`);
    return ts;
  };

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

  function selectCellRange() {
    useTableDataStore
      .getState()
      .setGridSelection(
        PANEL,
        selectionFromAnchorFocus(
          { rowIndex: 0, columnName: 'id' },
          { rowIndex: 1, columnName: 'name' },
          'cell',
        ),
      );
  }

  beforeEach(async () => {
    vi.resetModules();
    vi.clearAllMocks();
    mockDatabaseCommands.getTableData.mockResolvedValue(response);
    const mod = await import('../tableDataStore');
    useTableDataStore = mod.useTableDataStore;
    useTableDataStore.getState().reset();
  });

  it('cell mode + deleteRows([0,1]) keeps geometry, drops no rows and stages both deletes', async () => {
    await loadTable();
    selectCellRange();
    const before = loaded().gridSelection;
    expect(before.mode).toBe('cell');

    useTableDataStore.getState().deleteRows(PANEL, [0, 1]);

    expect(loaded().gridSelection).toEqual(before);
    expect(loaded().selectedRows.size).toBe(0);
    expect(loaded().lastSelectedIndex).toBeNull();
    // The delete scope comes from the explicit indices, not from any row set.
    expect(loaded().pendingChanges.size).toBe(2);
  });

  it('cell mode + deleteRows([]) is a true no-op', async () => {
    await loadTable();
    selectCellRange();
    const before = loaded().gridSelection;

    useTableDataStore.getState().deleteRows(PANEL, []);

    expect(loaded().gridSelection).toEqual(before);
    expect(loaded().selectedRows.size).toBe(0);
    expect(loaded().pendingChanges.size).toBe(0);
  });

  it('row mode + deleteRows of another row leaves the whole-row selection intact', async () => {
    await loadTable();
    useTableDataStore.getState().selectRow(PANEL, 0);

    useTableDataStore.getState().deleteRows(PANEL, [1]);

    expect(loaded().gridSelection.mode).toBe('row');
    expect(loaded().selectedRows).toEqual(new Set([0]));
    expect(loaded().lastSelectedIndex).toBe(0);
    expect(loaded().pendingChanges.size).toBe(1);
  });

  it('row mode + deleteRows of the selected row itself does not rewrite the selection', async () => {
    await loadTable();
    useTableDataStore.getState().selectRow(PANEL, 0);

    useTableDataStore.getState().deleteRows(PANEL, [0]);

    expect(loaded().gridSelection.mode).toBe('row');
    expect(loaded().selectedRows).toEqual(new Set([0]));
    expect(loaded().lastSelectedIndex).toBe(0);
    expect(loaded().pendingChanges.size).toBe(1);
  });

  it('"row mode with zero rows" is legal and does not fall back to none (§4 落地校正)', async () => {
    await loadTable();
    useTableDataStore.getState().selectRow(PANEL, 0);
    useTableDataStore.getState().selectRow(PANEL, 0); // clicking the only selected row clears it

    expect(loaded().gridSelection.mode).toBe('row');
    expect(loaded().selectedRows.size).toBe(0);
    expect(loaded().lastSelectedIndex).toBeNull();

    // The next row transition still works: no "empty => none" fallback may interfere.
    useTableDataStore.getState().setRowModeSelection(PANEL, 1, { range: true });
    expect(loaded().gridSelection.mode).toBe('row');
    expect(loaded().selectedRows).toEqual(new Set([1]));
    expect(loaded().lastSelectedIndex).toBe(1);
  });

  it('every public writer keeps the mode/row-set invariant', async () => {
    await loadTable();
    const assertInvariant = () => {
      const ts = loaded();
      if (ts.gridSelection.mode === 'none' || ts.gridSelection.mode === 'cell') {
        expect(ts.selectedRows.size).toBe(0);
        expect(ts.lastSelectedIndex).toBeNull();
      }
    };

    useTableDataStore.getState().selectRow(PANEL, 0);
    assertInvariant();
    expect(loaded().gridSelection.mode).toBe('row');

    useTableDataStore.getState().toggleSelectAll(PANEL);
    assertInvariant();
    expect(loaded().gridSelection.mode).toBe('row');

    useTableDataStore.getState().toggleSelectAll(PANEL);
    assertInvariant();
    expect(loaded().gridSelection.mode).toBe('row');
    expect(loaded().selectedRows.size).toBe(0);

    selectCellRange();
    assertInvariant();
    expect(loaded().gridSelection.mode).toBe('cell');

    useTableDataStore.getState().setGridSelection(PANEL, EMPTY_GRID_SELECTION);
    assertInvariant();
    expect(loaded().gridSelection).toEqual(EMPTY_GRID_SELECTION);

    useTableDataStore.getState().clearGridSelection(PANEL);
    assertInvariant();
    expect(loaded().gridSelection.mode).toBe('none');
  });
});
