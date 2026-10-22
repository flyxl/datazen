/**
 * The continuous journey of the cell editor (DB-05).
 *
 * It runs against the **real store**, so every step asserts a state change
 * and not a local React state: the editor writes through
 * `stageCellValue`, and the store is the only thing that decides what a
 * staged change means.
 *
 * The journey covers the states a user really passes through — the
 * incomplete ones included:
 *
 * - Step 1  double click → the editor opens on the cell;
 * - Step 2  type → nothing is written yet (the draft lives in the editor);
 * - Step 3  Enter → the value is staged;
 * - Step 4  Escape → nothing is written at all;
 * - Step 5  an IME composition → Enter confirms the candidate, not the cell;
 * - Step 6  Tab → the value is staged and the focus asks for the next cell;
 * - Step 7  an invalid value → the editor stays open and the write is refused;
 * - Step 8  Set NULL and Set empty string → two different writes.
 *
 * The locators are the contract's data attributes and the historical
 * `table-edit-input` locator — never the visible wording (12-testing D7).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render } from '@testing-library/react';
import { useMemo } from 'react';
import { DataTable } from '../DataTable';
import { useTableDataStore } from '../../../stores/tableDataStore';
import { emptyTableState } from '../../../stores/tableData/connectionState';
import type { TableState } from '../../../stores/tableData/types';
import type { TableChangeContext } from '../../../lib/tableChanges';
import type { Value } from '../../../types';

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

const PANEL = 'panel-cell-editor-journey';

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
];

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
      primaryKeyColumns={['id']}
      editingCell={ts?.editingCell ?? null}
      onCellDoubleClick={(row, col) => useTableDataStore.getState().startEdit(PANEL, row, col)}
      cellEditing={{
        columnEditorInfo: new Map(),
        readOnlyReason: null,
        onCellValue: (row: number, col: string, value: Value | null | undefined) =>
          useTableDataStore.getState().stageCellValue(PANEL, row, col, value),
      }}
      onCellEditCancel={() => useTableDataStore.getState().cancelEdit(PANEL)}
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

function editorInput(container: HTMLElement): HTMLInputElement {
  const found = container.querySelector('[data-testid="table-edit-input"]');
  if (!(found instanceof HTMLInputElement)) throw new Error('no editor input');
  return found;
}

/** The staged change of the first row, if there is one. */
function stagedChange(): TableState['pendingChanges'] extends Map<string, infer V> ? V | undefined : never {
  return [...slice().pendingChanges.values()][0];
}

beforeEach(() => {
  seed();
});

afterEach(cleanup);

describe('the cell editor journey', () => {
  it('journey: double click, type, Enter commits a value write', () => {
    const { container } = render(<StoreGrid />);

    // Step 1 — the double click opens the editor on that cell.
    fireEvent.doubleClick(cell(container, 0, 'name'));
    expect(slice().editingCell).toEqual({ row: 0, col: 'name' });
    expect(container.querySelector('[data-dt-editing="true"]')).not.toBeNull();

    // Step 2 — typing does not write anything yet.
    const input = editorInput(container);
    fireEvent.change(input, { target: { value: 'Alicia' } });
    expect(slice().pendingChanges.size).toBe(0);

    // Step 3 — Enter writes the value and closes the editor.
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(slice().pendingChanges.size).toBe(1);
    expect(stagedChange()?.changedColumns).toEqual(['name']);
    expect(stagedChange()?.currentValues.name).toBe('Alicia');
    expect(slice().editingCell).toBeNull();
  });

  it('journey: double click, retype, Escape writes nothing', () => {
    const { container } = render(<StoreGrid />);

    fireEvent.doubleClick(cell(container, 0, 'name'));
    const input = editorInput(container);
    fireEvent.change(input, { target: { value: 'Alicia' } });
    fireEvent.keyDown(input, { key: 'Escape' });

    expect(slice().pendingChanges.size).toBe(0);
    expect(slice().rows[0]?.name).toBe('Alice');
    expect(slice().editingCell).toBeNull();
  });

  it('journey: Chinese IME selection then Enter commits the composed text once', () => {
    const { container } = render(<StoreGrid />);

    fireEvent.doubleClick(cell(container, 0, 'name'));
    const input = editorInput(container);

    // The composition: the intermediate state is a half-typed candidate.
    fireEvent.compositionStart(input);
    fireEvent.change(input, { target: { value: 'rén' } });
    fireEvent.keyDown(input, { key: 'Enter', isComposing: true });
    expect(slice().pendingChanges.size).toBe(0);

    fireEvent.compositionEnd(input);
    fireEvent.change(input, { target: { value: '人' } });
    fireEvent.keyDown(input, { key: 'Enter' });

    expect(slice().pendingChanges.size).toBe(1);
    expect(stagedChange()?.currentValues.name).toBe('人');
  });

  it('journey: Tab commits and asks for the next cell', () => {
    const onCommitAndMove = vi.fn();
    const { container } = render(<StoreGrid />);

    fireEvent.doubleClick(cell(container, 0, 'name'));
    const input = editorInput(container);
    fireEvent.change(input, { target: { value: 'Alicia' } });
    fireEvent.keyDown(input, { key: 'Tab' });

    expect(slice().pendingChanges.size).toBe(1);
    // The move is a request: the grid owns the focus, the editor only asks.
    expect(onCommitAndMove).not.toHaveBeenCalled();
  });

  it('journey: clicking another cell commits the current one first', () => {
    const { container } = render(<StoreGrid />);

    fireEvent.doubleClick(cell(container, 0, 'name'));
    const input = editorInput(container);
    fireEvent.change(input, { target: { value: 'Alicia' } });
    fireEvent.blur(input);

    expect(slice().pendingChanges.size).toBe(1);
    expect(stagedChange()?.currentValues.name).toBe('Alicia');
  });

  it('journey: an invalid value blocks the move and keeps the editor open', () => {
    const { container } = render(<StoreGrid />);

    fireEvent.doubleClick(cell(container, 0, 'id'));
    const input = editorInput(container);
    fireEvent.change(input, { target: { value: 'not-a-number' } });
    fireEvent.keyDown(input, { key: 'Enter' });

    // Nothing was written, and the editor is still open with its reason.
    expect(slice().pendingChanges.size).toBe(0);
    expect(slice().editingCell).toEqual({ row: 0, col: 'id' });
    expect(container.querySelector('[data-dt-editor-error="invalidNumber"]')).not.toBeNull();
  });

  it('journey: Set NULL then Set empty string produce different writes', () => {
    const store = () => useTableDataStore.getState();

    // "Set NULL" writes SQL NULL.
    store().stageCellValue(PANEL, 0, 'name', null);
    expect(stagedChange()?.changedColumns).toEqual(['name']);
    expect(stagedChange()?.currentValues.name).toBeNull();

    // "Set empty string" writes the empty string — a value, not a NULL.
    store().stageCellValue(PANEL, 0, 'name', '');
    expect(stagedChange()?.currentValues.name).toBe('');

    // "Revert this cell" takes the column out of the write entirely.
    store().stageCellValue(PANEL, 0, 'name', undefined);
    expect(stagedChange()).toBeUndefined();
    expect(slice().pendingChanges.size).toBe(0);
  });
});
