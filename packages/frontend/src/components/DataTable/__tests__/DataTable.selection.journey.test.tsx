/**
 * DB-01 continuous journey: one session that clicks, extends, adds a disjoint cell, cancels
 * a drag, switches to the row gutter, edits a cell and finally clears the selection.
 *
 * The journey runs against the **real store** (`useTableDataStore`), so every step asserts
 * the mode/row-set pairing of 00-contracts §4.2 and not just a local React state:
 *
 * - `mode` is the single authority of the row set — `cell` mode always has an empty row set,
 *   `row` mode owns the rows and never carries geometry;
 * - the DOM is asserted at every step, because a selection that is correct in the store but
 *   invisible in the DOM is not wired up;
 * - the incomplete intermediate states are part of the journey: a drag that leaves the grid
 *   keeps its last valid coordinate, and an Escape in the middle of a drag cancels the whole
 *   session instead of letting the pointerup resurrect the region.
 */
import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { render, fireEvent, cleanup } from '@testing-library/react';
import { useMemo } from 'react';
import { DataTable } from '../DataTable';
import { useTableDataStore } from '../../../stores/tableDataStore';
import { emptyTableState } from '../../../stores/tableData/connectionState';
import type { TableState } from '../../../stores/tableData/types';
import { EMPTY_GRID_SELECTION } from '../../../stores/tableData/gridSelection';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../hooks/useColumnResize', () => ({
  useColumnResize: () => ({
    columnWidths: [160, 200],
    onResizeStart: vi.fn(),
  }),
  adjustWidthsForSort: (widths: number[]) => widths,
}));

vi.mock('../../../hooks/useVirtualTable', () => ({
  useVirtualTable: ({ rows }: { rows: unknown[][] }) => ({
    virtualRows: rows.map((_, index) => ({ index, key: String(index), start: index * 40 })),
    totalHeight: rows.length * 40,
  }),
}));

vi.mock('../../../lib/session/tableSession', () => ({ tableManualHandle: vi.fn(() => null) }));
vi.mock('../../../commands/database', () => ({
  databaseCommands: {
    getTableData: vi.fn(),
    previewPendingChanges: vi.fn(),
    commitPendingChanges: vi.fn(),
  },
}));

const PANEL = 'panel-grid-cell-journey';

const COLUMNS = [
  { name: 'id', dataType: 'integer', nullable: false, isPrimaryKey: true },
  { name: 'name', dataType: 'text', nullable: true },
];

const RECORDS = [
  { id: 1, name: 'Alice' },
  { id: 2, name: 'Bob' },
  { id: 3, name: 'Cara' },
];

/** Grid harness wired to the real store, exactly like `TableView` wires it. */
function StoreGrid() {
  const ts = useTableDataStore((s) => s.byPanel.get(PANEL));
  const columns = useMemo(
    () => (ts?.columns ?? []).map((c) => ({ id: c.name, name: c.name, type: c.dataType })),
    [ts],
  );
  const rows = useMemo(
    () => (ts?.rows ?? []).map((record) => columns.map((col) => record[col.name] ?? null)),
    [columns, ts],
  );

  return (
    <DataTable
      columns={columns}
      rows={rows}
      selectedRows={ts?.selectedRows ?? new Set<number>()}
      gridSelection={ts?.gridSelection ?? EMPTY_GRID_SELECTION}
      onGridSelectionChange={(selection) =>
        useTableDataStore.getState().setGridSelection(PANEL, selection)
      }
      onRowSelect={(index, opts) =>
        useTableDataStore.getState().setRowModeSelection(PANEL, index, opts)
      }
      onSelectAll={() => useTableDataStore.getState().toggleSelectAll(PANEL)}
      primaryKeyColumns={['id']}
      editingCell={ts?.editingCell ?? null}
      onCellDoubleClick={(row, col) => useTableDataStore.getState().startEdit(PANEL, row, col)}
      onCellEditCancel={() => useTableDataStore.getState().cancelEdit(PANEL)}
    />
  );
}

function slice(): TableState {
  const ts = useTableDataStore.getState().byPanel.get(PANEL);
  if (!ts) throw new Error('panel slice missing');
  return ts;
}

function cell(container: HTMLElement, rowIndex: number, columnName: string): HTMLElement {
  const found = container.querySelector(
    `[data-dt-row="${rowIndex}"][data-dt-col="${columnName}"]`,
  );
  if (!(found instanceof HTMLElement)) throw new Error(`cell ${rowIndex}/${columnName} not found`);
  return found;
}

/**
 * The selected **cells**. The row containers carry `data-dt-selected` as well
 * (DB-02: the row highlight needs a locator of its own), so a cell locator
 * has to name the cell attributes.
 */
function selectedCells(container: HTMLElement): HTMLElement[] {
  return Array.from(container.querySelectorAll('[data-dt-row][data-dt-selected="true"]'));
}

function surface(container: HTMLElement): HTMLElement {
  const found = container.querySelector('[data-dt-surface]');
  if (!(found instanceof HTMLElement)) throw new Error('grid surface not found');
  return found;
}

/** Click a cell the way the browser does: pointer down, then pointer up. */
function clickCell(
  container: HTMLElement,
  rowIndex: number,
  columnName: string,
  modifiers: { shiftKey?: boolean; metaKey?: boolean } = {},
) {
  const target = cell(container, rowIndex, columnName);
  fireEvent.pointerDown(target, { pointerId: 1, button: 0, ...modifiers });
  fireEvent.pointerUp(target, { pointerId: 1, button: 0, ...modifiers });
}

beforeEach(() => {
  vi.stubEnv('VITE_E2E', '1');
  const panel: TableState = {
    ...emptyTableState(),
    columns: COLUMNS,
    rows: RECORDS,
  };
  useTableDataStore.setState({ byPanel: new Map([[PANEL, panel]]) });
  // jsdom has no layout engine: the drag resolves its target through `elementFromPoint`.
  Object.defineProperty(document, 'elementFromPoint', {
    configurable: true,
    writable: true,
    value: vi.fn(() => null),
  });
});

afterEach(() => {
  cleanup();
  vi.unstubAllEnvs();
  vi.restoreAllMocks();
});

describe('DB-01 selection journey', () => {
  it('walks from a single cell to a region, a disjoint cell, the row gutter and back', () => {
    const { container } = render(<StoreGrid />);

    // ① A plain click enters cell mode: one marked cell, no row set.
    clickCell(container, 0, 'id');
    expect(slice().gridSelection.mode).toBe('cell');
    expect(slice().gridSelection.anchor).toEqual({ rowIndex: 0, columnName: 'id' });
    expect(slice().selectedRows.size).toBe(0);
    expect(selectedCells(container)).toHaveLength(1);
    expect(container.querySelector('[data-testid="data-table-selection-summary"]')).not.toBeNull();

    // ② Shift+click extends the rectangle from the anchor.
    clickCell(container, 1, 'name', { shiftKey: true });
    expect(slice().gridSelection.mode).toBe('cell');
    expect(slice().gridSelection.anchor).toEqual({ rowIndex: 0, columnName: 'id' });
    expect(slice().gridSelection.focus).toEqual({ rowIndex: 1, columnName: 'name' });
    expect(selectedCells(container)).toHaveLength(4);
    const summary = container.querySelector('[data-testid="data-table-selection-summary"]')!;
    expect(summary.getAttribute('data-dt-selection-rows')).toBe('2');
    expect(summary.getAttribute('data-dt-selection-cols')).toBe('2');
    expect(summary.getAttribute('data-dt-selection-cells')).toBe('4');

    // ③ Cmd+click adds a disjoint cell; the main region stays untouched.
    clickCell(container, 2, 'id', { metaKey: true });
    expect(slice().gridSelection.extraRanges).toHaveLength(1);
    expect(selectedCells(container)).toHaveLength(5);

    // ④ Escape leaves cell mode and clears every marked cell.
    fireEvent.keyDown(surface(container), { key: 'Escape' });
    expect(slice().gridSelection.mode).toBe('none');
    expect(slice().selectedRows.size).toBe(0);
    expect(selectedCells(container)).toHaveLength(0);
    expect(container.querySelector('[data-testid="data-table-selection-summary"]')).toBeNull();

    // ⑤ The row gutter switches to row mode: the row set appears, the geometry stays empty.
    const gutter = container.querySelectorAll('[role="rowheader"] button')[1] as HTMLButtonElement;
    fireEvent.click(gutter);
    expect(slice().gridSelection.mode).toBe('row');
    expect(slice().gridSelection.anchor).toBeNull();
    expect(slice().gridSelection.focus).toBeNull();
    expect(Array.from(slice().selectedRows)).toEqual([1]);
    expect(selectedCells(container)).toHaveLength(0);
    const row = container.querySelector('[data-dt-row-view="1"]')!;
    expect(row.getAttribute('aria-selected')).toBe('true');

    // ⑥ A cell click takes the selection back to cell mode and empties the row set:
    //    the row set may never survive next to a cell region (two truths).
    clickCell(container, 2, 'name');
    expect(slice().gridSelection.mode).toBe('cell');
    expect(slice().selectedRows.size).toBe(0);
    expect(selectedCells(container)).toHaveLength(1);
  });

  it('keeps the last valid coordinate when a drag leaves the grid', () => {
    const { container } = render(<StoreGrid />);

    fireEvent.pointerDown(cell(container, 0, 'id'), { pointerId: 1, button: 0 });
    // The pointer is over the second cell …
    vi.mocked(document.elementFromPoint).mockReturnValue(cell(container, 1, 'name'));
    fireEvent.pointerMove(surface(container), { pointerId: 1, clientX: 120, clientY: 40 });
    // … and then over the gap between two rows: no cell is hit, the last valid one is kept.
    vi.mocked(document.elementFromPoint).mockReturnValue(null);
    fireEvent.pointerMove(surface(container), { pointerId: 1, clientX: 160, clientY: 41 });

    fireEvent.pointerUp(surface(container), { pointerId: 1, clientX: 160, clientY: 41 });

    expect(slice().gridSelection.mode).toBe('cell');
    expect(slice().gridSelection.anchor).toEqual({ rowIndex: 0, columnName: 'id' });
    expect(slice().gridSelection.focus).toEqual({ rowIndex: 1, columnName: 'name' });
    expect(selectedCells(container)).toHaveLength(4);
  });

  it('cancels an in-flight drag on Escape and does not resurrect it on pointerup', () => {
    const { container } = render(<StoreGrid />);

    clickCell(container, 0, 'id');
    const markedBefore = selectedCells(container).length;

    fireEvent.pointerDown(cell(container, 1, 'name'), { pointerId: 1, button: 0 });
    vi.mocked(document.elementFromPoint).mockReturnValue(cell(container, 2, 'id'));
    fireEvent.pointerMove(surface(container), { pointerId: 1, clientX: 200, clientY: 80 });

    fireEvent.keyDown(surface(container), { key: 'Escape' });
    expect(slice().gridSelection.mode).toBe('none');
    expect(selectedCells(container)).toHaveLength(0);

    // The pointerup of the cancelled session must not create a new selection.
    fireEvent.pointerUp(surface(container), { pointerId: 1, clientX: 200, clientY: 80 });
    expect(slice().gridSelection.mode).toBe('none');
    expect(selectedCells(container)).toHaveLength(0);
    expect(markedBefore).toBe(1);
  });

  it('keeps the selection while a cell is edited and moves it afterwards', () => {
    const { container } = render(<StoreGrid />);

    clickCell(container, 0, 'name');
    expect(selectedCells(container)).toHaveLength(1);

    // Double click opens the editor; the selection survives the edit.
    fireEvent.doubleClick(cell(container, 0, 'name'));
    expect(container.querySelector('[data-testid="table-edit-input"]')).not.toBeNull();
    expect(slice().gridSelection.mode).toBe('cell');
    expect(selectedCells(container)).toHaveLength(1);

    // Escape belongs to the editor: the selection must not be cleared behind its back.
    fireEvent.keyDown(surface(container), { key: 'Escape' });
    expect(slice().gridSelection.mode).toBe('cell');
    expect(selectedCells(container)).toHaveLength(1);

    // A pointerdown on the editor input is ignored — it must not steal the selection.
    const input = container.querySelector('[data-testid="table-edit-input"]')!;
    fireEvent.pointerDown(input, { pointerId: 1, button: 0 });
    fireEvent.pointerUp(input, { pointerId: 1, button: 0 });
    expect(slice().gridSelection.anchor).toEqual({ rowIndex: 0, columnName: 'name' });

    // Leaving the editor and clicking another cell moves the selection.
    fireEvent.keyDown(input, { key: 'Escape' });
    clickCell(container, 1, 'id');
    expect(slice().gridSelection.mode).toBe('cell');
    expect(slice().gridSelection.anchor).toEqual({ rowIndex: 1, columnName: 'id' });
    expect(selectedCells(container)).toHaveLength(1);
  });
});
