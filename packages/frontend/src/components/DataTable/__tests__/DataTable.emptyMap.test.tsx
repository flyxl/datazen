/**
 * The empty-`columnEditorInfo` fallback (DB-05 ⑦).
 *
 * An **empty** map must mean "the owner knows nothing", exactly like a
 * missing one — so the grid falls back to `buildColumnEditorInfoFromDefs`
 * instead of losing every column's type.
 *
 * The question this file asks: does the fallback reach **all three** facts
 * the editor needs — the cell type, the nullability and the integer check?
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render } from '@testing-library/react';
import { useMemo } from 'react';
import { DataTable } from '../DataTable';
import { useTableDataStore } from '../../../stores/tableDataStore';
import { emptyTableState } from '../../../stores/tableData/connectionState';
import type { TableState } from '../../../stores/tableData/types';
import type { TableChangeContext } from '../../../lib/tableChanges';

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

const PANEL = 'panel-r2-empty-map';

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
  { name: 'qty', dataType: 'integer', nullable: false },
];

const RECORDS = [{ id: 1, qty: 5 }];

function seed(): void {
  const state: TableState = {
    ...emptyTableState(CONTEXT),
    columns: COLUMNS,
    rows: RECORDS,
    totalRows: RECORDS.length,
  };
  useTableDataStore.setState({ byPanel: new Map([[PANEL, state]]) });
}

/** The grid with an **empty** editor-info map (the case under test). */
function EmptyMapGrid() {
  const ts = useTableDataStore((s) => s.byPanel.get(PANEL));
  const columns = useMemo(
    () => (ts?.columns ?? []).map((c) => ({ id: c.name, name: c.name, type: c.dataType, nullable: c.nullable })),
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
        panelId: PANEL,
        columnEditorInfo: new Map(),
        readOnlyReason: null,
        onCellValue: (row: number, col: string, value) =>
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

beforeEach(() => seed());
afterEach(cleanup);

// The three cases ask the same question of the fallback: does it reach the
// cell type, the nullability **and** the integer check? (The owner hands
// the resolved facts to every cell, so all three answers come from one
// place.)
describe('an empty columnEditorInfo map means "the owner knows nothing"', () => {
  it('still renders data-dt-cell-type', () => {
    const { container } = render(<EmptyMapGrid />);
    const values = [...container.querySelectorAll('[data-dt-cell-type]')].map(
      (node) => node.getAttribute('data-dt-cell-type') ?? '',
    );
    expect(values.length).toBeGreaterThan(0);
    expect(new Set(values)).toEqual(new Set(['number']));
  });

  it('still refuses a fraction in an integer column', () => {
    const { container } = render(<EmptyMapGrid />);
    fireEvent.doubleClick(cell(container, 0, 'qty'));
    const input = container.querySelector('[data-testid="table-edit-input"]');
    expect(input).not.toBeNull();
    fireEvent.change(input!, { target: { value: '1.5' } });
    fireEvent.keyDown(input!, { key: 'Enter' });

    // An integer column must refuse a fraction.
    expect(container.querySelector('[data-dt-editor-error]')).not.toBeNull();
    expect(container.querySelector('[data-dt-editor-error]')?.getAttribute('data-dt-editor-error'))
      .toBe('invalidInteger');
    expect(useTableDataStore.getState().byPanel.get(PANEL)?.pendingChanges.size).toBe(0);
  });

  it('still refuses an empty input on a NOT NULL column', () => {
    const { container } = render(<EmptyMapGrid />);
    fireEvent.doubleClick(cell(container, 0, 'qty'));
    const input = container.querySelector('[data-testid="table-edit-input"]');
    expect(input).not.toBeNull();
    fireEvent.change(input!, { target: { value: '' } });
    fireEvent.keyDown(input!, { key: 'Enter' });

    expect(container.querySelector('[data-dt-editor-error]')?.getAttribute('data-dt-editor-error'))
      .toBe('emptyNotNull');
    expect(useTableDataStore.getState().byPanel.get(PANEL)?.pendingChanges.size).toBe(0);
  });
});
