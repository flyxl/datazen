/**
 * The re-read actions never drop the draft the user typed (DB-05).
 *
 * The user-visible failure this guards against: the user opens a cell, types
 * a value the editor cannot accept, and clicks "count exactly" — the panel
 * re-reads the page, the editor unmounts with the row, and the typing is
 * gone **without ever being written** (05 §4.4: "禁止静默丢弃用户的键入").
 *
 * The file drives the **real** editor, the **real** active-editor registry
 * and the **real** `useTableActions` together, and asserts the three things
 * the user would notice: the navigation is refused, the editor is still
 * open, and the draft is still in the input.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render } from '@testing-library/react';
import { useMemo } from 'react';
import { DataTable } from '../DataTable';
import { useTableActions } from '../../../windows/connection/useTableActions';
import { useTableDataStore } from '../../../stores/tableDataStore';
import { emptyTableState } from '../../../stores/tableData/connectionState';
import type { TableState } from '../../../stores/tableData/types';
import type { TableChangeContext } from '../../../lib/tableChanges';

const calls = vi.hoisted(() => ({
  reloadPanel: vi.fn(),
  loadTableData: vi.fn(),
}));

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

// The real store, with only the two re-reading actions replaced by recorders.
vi.mock('../../../stores/tableDataStore', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../../stores/tableDataStore')>();
  const real = actual.useTableDataStore;
  const wrapped = Object.assign(
    (selector: (s: unknown) => unknown) => real(selector),
    {
      getState: () => ({
        ...real.getState(),
        reloadPanel: calls.reloadPanel,
        loadTableData: calls.loadTableData,
      }),
      setState: real.setState,
      subscribe: real.subscribe,
    },
  );
  return { ...actual, useTableDataStore: wrapped };
});

const PANEL = 'panel-r2-data-loss';

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
  { name: 'qty', dataType: 'integer', nullable: true },
];

const RECORDS = [
  { id: 1, qty: 5 },
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

/** The grid plus the two buttons that re-read the page. */
function Harness() {
  const ts = useTableDataStore((s) => s.byPanel.get(PANEL));
  const columns = useMemo(
    () => (ts?.columns ?? []).map((c) => ({ id: c.name, name: c.name, type: c.dataType, nullable: c.nullable })),
    [ts],
  );
  const rows = useMemo(
    () => (ts?.rows ?? []).map((record) => columns.map((col) => record[col.name] ?? null)),
    [columns, ts],
  );
  const actions = useTableActions({
    panelId: PANEL,
    context: CONTEXT,
    invalidateNotice: () => undefined,
  });

  return (
    <div>
      <button type="button" data-testid="count-exact" onClick={() => actions.countExact()}>
        count
      </button>
      <button type="button" data-testid="retry-load" onClick={() => actions.load()}>
        load
      </button>
      <DataTable
        columns={columns}
        rows={rows}
        primaryKeyColumns={['id']}
        editingCell={ts?.editingCell ?? null}
        onCellDoubleClick={(row, col) => useTableDataStore.getState().startEdit(PANEL, row, col)}
        cellEditing={{
          panelId: PANEL,
          columnEditorInfo: new Map(),
          readOnlyReason: null,
          onCellValue: (row: number, col: string, value) =>
            useTableDataStore.getState().stageCellValue(PANEL, row, col, value),
        }}
        onCellEditCancel={() => useTableDataStore.getState().cancelEdit(PANEL)}
      />
    </div>
  );
}

function cell(container: HTMLElement, rowIndex: number, columnName: string): HTMLElement {
  const found = container.querySelector(
    `[data-dt-row="${rowIndex}"][data-dt-col="${columnName}"]`,
  );
  if (!(found instanceof HTMLElement)) throw new Error(`cell ${rowIndex}/${columnName} not found`);
  return found;
}

beforeEach(() => {
  calls.reloadPanel.mockClear();
  calls.loadTableData.mockClear();
  seed();
});

afterEach(cleanup);

describe('verify: the re-read actions never drop the draft the user typed', () => {
  it('refuses the exact-count re-read and keeps the editor and the draft', () => {
    const { container } = render(<Harness />);

    // The user opens the cell and types a value the editor refuses.
    fireEvent.doubleClick(cell(container, 0, 'qty'));
    const input = container.querySelector('[data-testid="table-edit-input"]');
    expect(input).not.toBeNull();
    fireEvent.change(input!, { target: { value: 'not-a-number' } });
    expect(container.querySelector('[data-dt-editing="true"]')).not.toBeNull();

    // The user clicks "count exactly": the panel would re-read the page.
    fireEvent.click(container.querySelector('[data-testid="count-exact"]')!);

    // 1. the navigation is refused
    expect(calls.reloadPanel).not.toHaveBeenCalled();
    // 2. the editor is still open
    expect(container.querySelector('[data-dt-editing="true"]')).not.toBeNull();
    // 3. the draft is still there
    expect((input as HTMLInputElement).value).toBe('not-a-number');
    // 4. and nothing was written
    expect(slice().pendingChanges.size).toBe(0);
  });

  it('refuses the retry load and keeps the editor and the draft', () => {
    const { container } = render(<Harness />);

    fireEvent.doubleClick(cell(container, 0, 'qty'));
    const input = container.querySelector('[data-testid="table-edit-input"]');
    fireEvent.change(input!, { target: { value: 'not-a-number' } });

    fireEvent.click(container.querySelector('[data-testid="retry-load"]')!);

    expect(calls.loadTableData).not.toHaveBeenCalled();
    expect(container.querySelector('[data-dt-editing="true"]')).not.toBeNull();
    expect((input as HTMLInputElement).value).toBe('not-a-number');
    expect(slice().pendingChanges.size).toBe(0);
  });

  it('still lets a valid draft through (the guard is not a blanket block)', () => {
    const { container } = render(<Harness />);

    fireEvent.doubleClick(cell(container, 0, 'qty'));
    const input = container.querySelector('[data-testid="table-edit-input"]');
    fireEvent.change(input!, { target: { value: '7' } });

    fireEvent.click(container.querySelector('[data-testid="count-exact"]')!);

    // The draft is acceptable, so it is written and the re-read may go on.
    expect(slice().pendingChanges.size).toBe(1);
    expect(calls.reloadPanel).toHaveBeenCalled();
  });
});
