/**
 * DB-02: the continuous journey of the grid keyboard.
 *
 * It runs against the **real store** (`useTableDataStore`), so every step
 * asserts a state transition and not a local React state: the keyboard writes
 * through `setGridSelection` / `selectRow` / `toggleSelectAll`, and the store
 * is the only thing that decides what a selection means.
 *
 * The journey covers the states a user really passes through — the incomplete
 * ones included:
 *
 * - a mouse start, a keyboard extension, a disjoint range, `Cmd+A` and `Escape`;
 * - a `Shift`+click that hands the extension over to the keyboard;
 * - an open editor, which owns the keys while it is open;
 * - an IME composition, which owns `Enter` and the first `Escape`;
 * - `Delete`, which is a no-op in the cell mode and a row action in the row mode;
 * - a hidden column, which invalidates the focus;
 * - the border, where a repeated key must not collapse the selection.
 *
 * The locators are the contract's data attributes — never the visible wording
 * (12-testing D7).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render } from '@testing-library/react';
import { useMemo } from 'react';
import { DataTable } from '../DataTable';
import { useTableDataStore } from '../../../stores/tableDataStore';
import { emptyTableState } from '../../../stores/tableData/connectionState';
import type { TableState } from '../../../stores/tableData/types';
import type { TableChangeContext } from '../../../lib/tableChanges';
import type { Value } from '../../../types';
import { EMPTY_GRID_SELECTION, type GridSelection } from '../../../stores/tableData/gridSelection';

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

const PANEL = 'panel-grid-keyboard-journey';

const CONTEXT: TableChangeContext = {
  connectionId: 'conn-1',
  dbSessionId: 'sess-1',
  driverType: 'postgresql',
  database: 'app',
  schema: null,
  table: 'users',
};

const COLUMNS = [
  { name: 'id', dataType: 'integer', nullable: false, isPrimaryKey: true },
  { name: 'name', dataType: 'text', nullable: true },
];

const RECORDS = [
  { id: 1, name: 'Alice' },
  { id: 2, name: 'Bob' },
  { id: 3, name: 'Cara' },
  { id: 4, name: 'Dan' },
  { id: 5, name: 'Eve' },
  { id: 6, name: 'Fay' },
  { id: 7, name: 'Gus' },
  { id: 8, name: 'Hal' },
];

/** The row deletions the grid asked for (the delete action itself is 06's). */
const deleteRows = vi.fn();
/** The cells the editor was opened for — the grid may open it only once. */
const editStarts: string[] = [];
/** Every selection the grid wrote back — the counter of the write path. */
const selectionWrites: GridSelection[] = [];

function seed(): void {
  const state: TableState = {
    ...emptyTableState(CONTEXT),
    columns: COLUMNS,
    rows: RECORDS,
    totalRows: RECORDS.length,
  };
  useTableDataStore.setState({ byPanel: new Map([[PANEL, state]]) });
}

function slice(): TableState {
  const ts = useTableDataStore.getState().byPanel.get(PANEL);
  if (!ts) throw new Error('panel slice missing');
  return ts;
}

/** The grid, wired to the real store exactly like `TableView` wires it. */
function StoreGrid({ keyboardNavigation = true }: { keyboardNavigation?: boolean }) {
  const ts = useTableDataStore((s) => s.byPanel.get(PANEL));
  const columns = useMemo(() => {
    const all = ts?.columns ?? [];
    const visible = ts?.visibleColumns ?? null;
    const shown =
      visible === null ? all : all.filter((column) => visible.includes(column.name));
    return shown.map((c) => ({
      id: c.name,
      name: c.name,
      type: c.dataType,
      nullable: c.nullable,
    }));
  }, [ts]);
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
      onGridSelectionChange={(next) => {
        selectionWrites.push(next);
        useTableDataStore.getState().setGridSelection(PANEL, next);
      }}
      onRowSelect={(index, opts) => useTableDataStore.getState().selectRow(PANEL, index, opts)}
      onSelectAll={() => useTableDataStore.getState().toggleSelectAll(PANEL)}
      lastSelectedIndex={ts?.lastSelectedIndex ?? null}
      keyboardNavigation={keyboardNavigation}
      primaryKeyColumns={['id']}
      editingCell={ts?.editingCell ?? null}
      onCellDoubleClick={(row, col) => {
        editStarts.push(`${row}:${col}`);
        useTableDataStore.getState().startEdit(PANEL, row, col);
      }}
      cellEditing={{
        panelId: PANEL,
        columnEditorInfo: new Map(),
        readOnlyReason: null,
        onCellValue: (row: number, col: string, value: Value | null | undefined) =>
          useTableDataStore.getState().stageCellValue(PANEL, row, col, value),
      }}
      onCellEditCancel={() => useTableDataStore.getState().cancelEdit(PANEL)}
      onDeleteRows={deleteRows}
    />
  );
}

function cell(container: HTMLElement, rowIndex: number, columnName: string): HTMLElement {
  const found = container.querySelector(
    `[data-dt-row="${rowIndex}"][data-dt-col="${columnName}"]`,
  );
  if (!(found instanceof HTMLElement)) throw new Error(`cell ${rowIndex}/${columnName} not found`);
  return found;
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
  seed();
  deleteRows.mockClear();
  editStarts.length = 0;
  selectionWrites.length = 0;
  // jsdom has no layout engine: the pointer resolves its target through `elementFromPoint`.
  Object.defineProperty(document, 'elementFromPoint', {
    configurable: true,
    writable: true,
    value: vi.fn(() => null),
  });
});

afterEach(cleanup);

describe('the grid keyboard journey', () => {
  it('journey: mouse start → keyboard extension → Cmd+A closes → Escape resets', () => {
    const { container } = render(<StoreGrid />);

    // ① A click enters the cell mode: one cell, no row set.
    clickCell(container, 2, 'id');
    expect(slice().gridSelection.mode).toBe('cell');
    expect(slice().gridSelection.anchor).toEqual({ rowIndex: 2, columnName: 'id' });
    expect(slice().selectedRows.size).toBe(0);

    // ② Shift+ArrowDown extends the region from the anchor.
    fireEvent.keyDown(surface(container), { key: 'ArrowDown', shiftKey: true });
    expect(slice().gridSelection.focus).toEqual({ rowIndex: 3, columnName: 'id' });
    expect(slice().gridSelection.anchor).toEqual({ rowIndex: 2, columnName: 'id' });

    // ③ Shift+ArrowRight adds the next column of the render order.
    fireEvent.keyDown(surface(container), { key: 'ArrowRight', shiftKey: true });
    expect(slice().gridSelection.focus).toEqual({ rowIndex: 3, columnName: 'name' });
    expect(slice().gridSelection.extraRanges).toHaveLength(0);

    // ④ Cmd+click adds a disjoint cell.
    clickCell(container, 5, 'id', { metaKey: true });
    expect(slice().gridSelection.extraRanges).toHaveLength(1);

    // ⑤ Shift+ArrowDown keeps the disjoint range (the incomplete state).
    fireEvent.keyDown(surface(container), { key: 'ArrowDown', shiftKey: true });
    expect(slice().gridSelection.extraRanges).toHaveLength(1);
    expect(slice().gridSelection.focus).toEqual({ rowIndex: 4, columnName: 'name' });
    expect(slice().gridSelection.anchor).toEqual({ rowIndex: 2, columnName: 'id' });

    // ⑥ An unmodified arrow collapses the main region and drops the extra ranges.
    fireEvent.keyDown(surface(container), { key: 'ArrowDown' });
    expect(slice().gridSelection.extraRanges).toHaveLength(0);
    expect(slice().gridSelection.anchor).toEqual({ rowIndex: 5, columnName: 'name' });
    expect(slice().gridSelection.focus).toEqual({ rowIndex: 5, columnName: 'name' });

    // ⑦ Cmd+A selects every row of the page: the row mode owns the row set.
    fireEvent.keyDown(surface(container), { key: 'a', metaKey: true });
    expect(slice().gridSelection.mode).toBe('row');
    expect(slice().gridSelection.anchor).toBeNull();
    expect(slice().selectedRows.size).toBe(RECORDS.length);
    expect(container.querySelectorAll('[data-dt-row-view][data-dt-selected="true"]').length).toBe(
      RECORDS.length,
    );

    // ⑦b The second `Cmd+A` clears the rows again (the existing toggle semantics).
    fireEvent.keyDown(surface(container), { key: 'a', metaKey: true });
    expect(slice().selectedRows.size).toBe(0);
    expect(slice().gridSelection.mode).toBe('row');

    // ⑧ Escape clears everything.
    fireEvent.keyDown(surface(container), { key: 'Escape' });
    expect(slice().gridSelection.mode).toBe('none');
    expect(slice().selectedRows.size).toBe(0);

    // ⑨ The next arrow lands on the remembered cursor, not on the first row.
    fireEvent.keyDown(surface(container), { key: 'ArrowDown' });
    expect(slice().gridSelection.mode).toBe('cell');
    expect(slice().gridSelection.focus).toEqual({ rowIndex: 5, columnName: 'name' });
    expect(container.querySelector('[data-dt-active="true"]')?.getAttribute('data-dt-row')).toBe(
      '5',
    );
  });

  it('journey: a Shift+click hands the extension over to the keyboard without moving the anchor', () => {
    const { container } = render(<StoreGrid />);

    clickCell(container, 1, 'id');
    fireEvent.keyDown(surface(container), { key: 'ArrowDown', shiftKey: true });
    expect(slice().gridSelection.focus).toEqual({ rowIndex: 2, columnName: 'id' });
    expect(slice().gridSelection.anchor).toEqual({ rowIndex: 1, columnName: 'id' });

    // The Shift+click moves the focus corner, never the anchor.
    clickCell(container, 6, 'id', { shiftKey: true });
    expect(slice().gridSelection.anchor).toEqual({ rowIndex: 1, columnName: 'id' });
    expect(slice().gridSelection.focus).toEqual({ rowIndex: 6, columnName: 'id' });

    fireEvent.keyDown(surface(container), { key: 'ArrowDown', shiftKey: true });
    expect(slice().gridSelection.focus).toEqual({ rowIndex: 7, columnName: 'id' });
    expect(slice().gridSelection.anchor).toEqual({ rowIndex: 1, columnName: 'id' });
  });

  it('journey: Enter opens the editor, the editor owns the keys, Escape cancels without clearing the selection', () => {
    const { container } = render(<StoreGrid />);

    clickCell(container, 0, 'name');
    expect(slice().gridSelection.mode).toBe('cell');

    // ① Enter opens the editor on the focused cell.
    fireEvent.keyDown(surface(container), { key: 'Enter' });
    expect(slice().editingCell).toEqual({ row: 0, col: 'name' });
    const input = container.querySelector('[data-testid="table-edit-input"]');
    expect(input).not.toBeNull();

    // ② Inside the editor the arrow key belongs to the editor: the cursor stays.
    fireEvent.change(input!, { target: { value: 'Alicia' } });
    fireEvent.keyDown(input!, { key: 'ArrowDown' });
    expect(slice().gridSelection.focus).toEqual({ rowIndex: 0, columnName: 'name' });
    expect(slice().gridSelection.mode).toBe('cell');

    // ③ Escape cancels the edit and leaves the selection alone.
    fireEvent.keyDown(input!, { key: 'Escape' });
    expect(slice().editingCell).toBeNull();
    expect(slice().gridSelection.mode).toBe('cell');
    expect(slice().gridSelection.anchor).toEqual({ rowIndex: 0, columnName: 'name' });

    // ④ Enter again, type, Tab: the editor commits (05 owns the editing keys).
    fireEvent.keyDown(surface(container), { key: 'Enter' });
    const second = container.querySelector('[data-testid="table-edit-input"]');
    expect(second).not.toBeNull();
    fireEvent.change(second!, { target: { value: 'Alicia' } });
    fireEvent.keyDown(second!, { key: 'Tab' });
    expect(slice().pendingChanges.size).toBe(1);
    expect(slice().gridSelection.mode).toBe('cell');
  });

  it('journey: an IME composition owns Enter and the first Escape', () => {
    const { container } = render(<StoreGrid />);

    clickCell(container, 1, 'id');
    expect(slice().gridSelection.mode).toBe('cell');

    // ① The combination key belongs to the IME.
    fireEvent.keyDown(surface(container), { key: 'Process', isComposing: true });
    expect(slice().gridSelection.focus).toEqual({ rowIndex: 1, columnName: 'id' });

    // ② Enter confirms the candidate word — it must not open the editor.
    fireEvent.keyDown(surface(container), { key: 'Enter', isComposing: true });
    expect(slice().editingCell).toBeNull();
    expect(slice().gridSelection.mode).toBe('cell');

    // ③ The first Escape closes the candidate window, not the selection.
    fireEvent.keyDown(surface(container), { key: 'Escape', isComposing: true });
    expect(slice().gridSelection.mode).toBe('cell');

    // ④ The second Escape clears the selection.
    fireEvent.keyDown(surface(container), { key: 'Escape' });
    expect(slice().gridSelection.mode).toBe('none');
  });

  it('journey: Delete is a no-op in the cell mode and a row action in the row mode', () => {
    const { container } = render(<StoreGrid />);

    clickCell(container, 0, 'id');
    fireEvent.keyDown(surface(container), { key: 'ArrowDown', shiftKey: true });
    fireEvent.keyDown(surface(container), { key: 'ArrowDown', shiftKey: true });
    expect(slice().gridSelection.focus).toEqual({ rowIndex: 2, columnName: 'id' });
    // The cell mode keeps the row set empty (00-contracts §4.2, rule 3).
    expect(slice().selectedRows.size).toBe(0);

    // ① In the cell mode the row set is empty: Delete does nothing.
    fireEvent.keyDown(surface(container), { key: 'Delete' });
    expect(deleteRows).not.toHaveBeenCalled();

    // ② Cmd+A switches to the row mode: Delete now acts on the selected rows.
    fireEvent.keyDown(surface(container), { key: 'a', metaKey: true });
    fireEvent.keyDown(surface(container), { key: 'Delete' });
    expect(deleteRows).toHaveBeenCalledTimes(1);
    expect(deleteRows.mock.calls[0]?.[0]).toEqual(RECORDS.map((_, index) => index));
  });

  it('journey: a hidden column invalidates the focus and the next arrow does not return to it', () => {
    const { container } = render(<StoreGrid />);

    clickCell(container, 1, 'name');
    expect(slice().gridSelection.focus).toEqual({ rowIndex: 1, columnName: 'name' });

    // The hidden column takes the focus with it.
    act(() => {
      useTableDataStore.getState().setVisibleColumns(PANEL, ['id']);
    });
    expect(slice().gridSelection.mode).toBe('none');

    // The next arrow lands on the first **visible** column, never on the ghost.
    fireEvent.keyDown(surface(container), { key: 'ArrowDown' });
    expect(slice().gridSelection.mode).toBe('cell');
    expect(slice().gridSelection.focus).toEqual({ rowIndex: 0, columnName: 'id' });
  });

  it('journey: repeated arrows at the border keep the selection', () => {
    const { container } = render(<StoreGrid />);

    clickCell(container, 0, 'id');
    const before = slice().gridSelection;
    selectionWrites.length = 0;

    for (let i = 0; i < 5; i += 1) {
      fireEvent.keyDown(surface(container), { key: 'ArrowUp' });
      fireEvent.keyDown(surface(container), { key: 'ArrowLeft' });
    }

    expect(slice().gridSelection).toEqual(before);
    // Not a single write: the border is a no-op, not a collapse.
    expect(selectionWrites).toHaveLength(0);
  });
});

describe('the grid keyboard at the surface', () => {
  it('intercepts no key at all when the navigation is off', () => {
    const { container } = render(<StoreGrid keyboardNavigation={false} />);

    expect(surface(container).getAttribute('tabindex')).toBe('0');
    fireEvent.keyDown(surface(container), { key: 'ArrowDown' });
    expect(slice().gridSelection.mode).toBe('none');
    expect(container.querySelector('[data-dt-active="true"]')).toBeNull();
  });

  it('lets the undo shortcut through untouched', () => {
    const { container } = render(<StoreGrid />);
    clickCell(container, 0, 'id');
    selectionWrites.length = 0;

    // `fireEvent` reports `false` when the default was prevented.
    expect(fireEvent.keyDown(surface(container), { key: 'z', metaKey: true })).toBe(true);
    expect(selectionWrites).toHaveLength(0);
  });

  it('claims Tab so that the focus leaves the grid', () => {
    const { container } = render(<StoreGrid />);
    clickCell(container, 0, 'id');
    selectionWrites.length = 0;

    expect(fireEvent.keyDown(surface(container), { key: 'Tab' })).toBe(false);
    expect(selectionWrites).toHaveLength(0);
  });

  it('takes the row out of the tab order and opens the editor exactly once', () => {
    const { container } = render(<StoreGrid />);

    // Roving tabindex: the scroll container is the only tab stop of the grid.
    expect(container.querySelector('[data-dt-row-view]')?.getAttribute('tabindex')).toBe('-1');

    clickCell(container, 0, 'name');
    fireEvent.keyDown(surface(container), { key: 'Enter' });

    // `Enter` opens the editor — once. The row does not select itself as well:
    // that would be two meanings for one key (DB-02 §4.4).
    expect(editStarts).toEqual(['0:name']);
    expect(slice().editingCell).toEqual({ row: 0, col: 'name' });
  });

  it('moves the cursor with the arrow keys and writes the selection back', () => {
    const { container } = render(<StoreGrid />);

    clickCell(container, 1, 'id');
    fireEvent.keyDown(surface(container), { key: 'ArrowDown' });
    expect(slice().gridSelection.focus).toEqual({ rowIndex: 2, columnName: 'id' });
    expect(slice().gridSelection.anchor).toEqual({ rowIndex: 2, columnName: 'id' });
    expect(container.querySelector('[data-dt-active="true"]')?.getAttribute('data-dt-col')).toBe(
      'id',
    );

    // The row mode moves the row selection instead of a cell.
    clickCell(container, 0, 'id');
    fireEvent.keyDown(surface(container), { key: 'ArrowDown' });
    expect(slice().gridSelection.mode).toBe('cell');
  });
});
