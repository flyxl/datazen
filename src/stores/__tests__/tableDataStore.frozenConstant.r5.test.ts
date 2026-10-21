/**
 * DB-01 round-5 coverage for GS4-D1: the shared `ROW_MODE_GRID_SELECTION` constant is frozen
 * exactly like `EMPTY_GRID_SELECTION`, so the reference stored in every panel slice cannot be
 * mutated by a stray consumer write.
 *
 * `Object.isFrozen` alone is not enough — a frozen object can still be `Reflect.set`-ed into
 * silently, and freezing the object without freezing `extraRanges` leaves `.push` working.
 * These cases therefore attempt the four real writes and require a thrown `TypeError`, then
 * verify the canonical shape survived, and finally prove the three store writers still hand
 * back the very same frozen object (no defensive copy, no silent mutation).
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { GridSelection } from '../tableData/gridSelection';
import type { TableState } from '../tableData/types';

let EMPTY_GRID_SELECTION: GridSelection;
let ROW_MODE_GRID_SELECTION: GridSelection;

const mockDatabaseCommands = {
  getTableData: vi.fn(),
  previewPendingChanges: vi.fn(),
  commitPendingChanges: vi.fn(),
};

vi.mock('../../lib/session/tableSession', () => ({ tableManualHandle: vi.fn(() => null) }));
vi.mock('../../commands/database', () => ({ databaseCommands: mockDatabaseCommands }));

const PANEL_A = 'panel-freeze-r5-a';
const PANEL_B = 'panel-freeze-r5-b';

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

const CANONICAL_ROW = '{"anchor":null,"focus":null,"extraRanges":[],"mode":"row"}';
const CANONICAL_EMPTY = '{"anchor":null,"focus":null,"extraRanges":[],"mode":"none"}';

const RANGE = {
  start: { rowIndex: 0, columnName: 'id' },
  end: { rowIndex: 0, columnName: 'id' },
};

/** Run a write that must throw; return the actual error name and message for the report. */
function captureThrow(write: () => void): { name: string; message: string } {
  try {
    write();
  } catch (error) {
    const typed = error as Error;
    return { name: typed.name, message: typed.message };
  }
  throw new Error('expected the write to throw, but it returned normally');
}

/** The four write attempts a stray consumer could make against a shared constant. */
function writesAgainst(selection: GridSelection): Array<{ label: string; run: () => void }> {
  return [
    { label: 'extraRanges.push', run: () => selection.extraRanges.push({ ...RANGE }) },
    {
      label: 'extraRanges[0] =',
      run: () => {
        selection.extraRanges[0] = { ...RANGE };
      },
    },
    { label: 'Object.assign(mode)', run: () => Object.assign(selection, { mode: 'none' }) },
    {
      label: 'selection.anchor =',
      run: () => {
        selection.anchor = { rowIndex: 1, columnName: 'name' };
      },
    },
  ];
}

describe('frozen shared selection constants (round 5)', () => {
  let useTableDataStore: typeof import('../tableDataStore').useTableDataStore;

  const loaded = (panelId: string): TableState => {
    const ts = useTableDataStore.getState().byPanel.get(panelId);
    if (!ts) throw new Error(`panel ${panelId} has no table-data slice`);
    return ts;
  };

  async function loadTable(panelId: string) {
    await useTableDataStore.getState().loadTableData({
      panelId,
      dbSessionId: 'conn-1',
      table: 'users',
      connectionId: 'conn-1',
      driverType: 'postgres',
      database: 'app',
      schema: null,
    });
  }

  beforeEach(async () => {
    vi.resetModules();
    vi.clearAllMocks();
    mockDatabaseCommands.getTableData.mockResolvedValue(response);
    const mod = await import('../tableDataStore');
    useTableDataStore = mod.useTableDataStore;
    const gridSelection = await import('../tableData/gridSelection');
    const selectionActions = await import('../tableData/selectionActions');
    EMPTY_GRID_SELECTION = gridSelection.EMPTY_GRID_SELECTION;
    ROW_MODE_GRID_SELECTION = selectionActions.ROW_MODE_GRID_SELECTION;
    useTableDataStore.getState().reset();
  });

  it('the row-mode constant and its range list are frozen, with the canonical shape', () => {
    expect(Object.isFrozen(ROW_MODE_GRID_SELECTION)).toBe(true);
    expect(Object.isFrozen(ROW_MODE_GRID_SELECTION.extraRanges)).toBe(true);
    expect(JSON.stringify(ROW_MODE_GRID_SELECTION)).toBe(CANONICAL_ROW);
    expect(ROW_MODE_GRID_SELECTION.extraRanges).toHaveLength(0);
  });

  it('every write against the row-mode constant throws TypeError', () => {
    for (const { label, run } of writesAgainst(ROW_MODE_GRID_SELECTION)) {
      const caught = captureThrow(run);
      expect(caught.name, `${label} must throw TypeError`).toBe('TypeError');
      expect(caught.message, `${label} must explain the failure`).toMatch(/extensible|read only/i);
    }
  });

  it('failed writes leave the row-mode constant byte-identical', () => {
    for (const { run } of writesAgainst(ROW_MODE_GRID_SELECTION)) {
      captureThrow(run);
    }
    expect(JSON.stringify(ROW_MODE_GRID_SELECTION)).toBe(CANONICAL_ROW);
    expect(ROW_MODE_GRID_SELECTION.anchor).toBeNull();
    expect(ROW_MODE_GRID_SELECTION.focus).toBeNull();
    expect(ROW_MODE_GRID_SELECTION.mode).toBe('row');
    expect(ROW_MODE_GRID_SELECTION.extraRanges).toHaveLength(0);
  });

  it('EMPTY_GRID_SELECTION behaves identically for all four writes', () => {
    expect(Object.isFrozen(EMPTY_GRID_SELECTION)).toBe(true);
    expect(Object.isFrozen(EMPTY_GRID_SELECTION.extraRanges)).toBe(true);
    for (const { label, run } of writesAgainst(EMPTY_GRID_SELECTION)) {
      const caught = captureThrow(run);
      expect(caught.name, `${label} must throw TypeError on EMPTY too`).toBe('TypeError');
    }
    expect(JSON.stringify(EMPTY_GRID_SELECTION)).toBe(CANONICAL_EMPTY);
  });

  it('the three writers still store the same frozen object (no defensive copy)', async () => {
    await loadTable(PANEL_A);
    await loadTable(PANEL_B);

    // writer 1: applyRowModeTransition (selectRow)
    useTableDataStore.getState().selectRow(PANEL_A, 0);
    expect(loaded(PANEL_A).gridSelection).toBe(ROW_MODE_GRID_SELECTION);

    // writer 2: applyGridSelection row branch (setGridSelection with dirty geometry)
    useTableDataStore.getState().setGridSelection(PANEL_B, {
      anchor: { rowIndex: 0, columnName: 'id' },
      focus: { rowIndex: 1, columnName: 'name' },
      extraRanges: [{ ...RANGE }],
      mode: 'row',
    });
    expect(loaded(PANEL_B).gridSelection).toBe(ROW_MODE_GRID_SELECTION);

    // writer 3: toggleSelectAll
    useTableDataStore.getState().toggleSelectAll(PANEL_A);
    expect(loaded(PANEL_A).gridSelection).toBe(ROW_MODE_GRID_SELECTION);

    // Both panels hold the very same frozen instance, and it is still pristine.
    expect(loaded(PANEL_A).gridSelection).toBe(loaded(PANEL_B).gridSelection);
    expect(JSON.stringify(ROW_MODE_GRID_SELECTION)).toBe(CANONICAL_ROW);
  });

  it('mutating through a stored panel reference throws and cannot leak to another panel', async () => {
    await loadTable(PANEL_A);
    await loadTable(PANEL_B);
    useTableDataStore.getState().selectRow(PANEL_A, 0);
    useTableDataStore.getState().selectRow(PANEL_B, 1);

    const stored = loaded(PANEL_A).gridSelection;
    const caught = captureThrow(() => stored.extraRanges.push({ ...RANGE }));
    expect(caught.name).toBe('TypeError');

    expect(JSON.stringify(loaded(PANEL_B).gridSelection)).toBe(CANONICAL_ROW);
    expect(loaded(PANEL_B).selectedRows).toEqual(new Set([1]));
    expect(loaded(PANEL_A).selectedRows).toEqual(new Set([0]));
  });
});
