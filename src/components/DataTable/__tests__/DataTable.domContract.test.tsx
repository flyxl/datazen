/**
 * The DOM contract of the grid (DB-05).
 *
 * 05 §4.4 / §11 #14: `data-dt-cell-type` carries a **normalised `CellType`**
 * (one of eleven values, never free text), and DB-05 owns three more
 * attributes: `data-dt-editing`, `data-dt-null` and `data-dt-readonly`.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render } from '@testing-library/react';
import { useMemo } from 'react';
import { DataTable } from '../DataTable';
import { useTableDataStore } from '../../../stores/tableDataStore';
import { emptyTableState } from '../../../stores/tableData/connectionState';
import { buildColumnEditorInfo } from '../../../lib/cellEditorKind';
import type { CellTypeHints } from '../../../lib/cellTypes';
import type { CellType } from '../../../lib/cellTypes';
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

const PANEL = 'panel-dom-contract';

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
  { name: 'tags', dataType: 'jsonb', nullable: true },
  { name: 'created', dataType: 'timestamp without time zone', nullable: true },
  { name: 'flag', dataType: 'boolean', nullable: true },
];

const RECORDS = [
  { id: 1, name: 'Alice', tags: { a: 1 }, created: '2026-01-01 10:00:00', flag: true },
  { id: 2, name: null, tags: null, created: null, flag: null },
];

const ALL_CELL_TYPES: readonly CellType[] = [
  'text',
  'number',
  'bool',
  'date',
  'time',
  'datetime',
  'json',
  'bytes',
  'enum',
  'uuid',
  'unknown',
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
  const columnEditorInfo = useMemo(
    () => buildColumnEditorInfo(ts?.columns ?? []),
    [ts],
  );

  return (
    <DataTable
      columns={columns}
      rows={rows}
      primaryKeyColumns={['id']}
      editingCell={ts?.editingCell ?? null}
      onCellDoubleClick={(row, col) => useTableDataStore.getState().startEdit(PANEL, row, col)}
      cellEditing={{
        columnEditorInfo,
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

describe('the DOM contract of the grid', () => {
  it('renders data-dt-cell-type with a normalised cell type, never free text', () => {
    const { container } = render(<StoreGrid />);
    const values = [...container.querySelectorAll('[data-dt-cell-type]')].map(
      (node) => node.getAttribute('data-dt-cell-type') ?? '',
    );
    expect(values.length).toBeGreaterThan(0);
    for (const value of values) {
      expect(ALL_CELL_TYPES, value).toContain(value as CellType);
    }
    // The five columns of the fixture, once each.
    expect(new Set(values)).toEqual(new Set(['number', 'text', 'json', 'datetime', 'bool']));
  });

  it('marks the editing cell with data-dt-editing', () => {
    const { container } = render(<StoreGrid />);
    expect(container.querySelector('[data-dt-editing="true"]')).toBeNull();
    fireEvent.doubleClick(cell(container, 0, 'name'));
    expect(container.querySelector('[data-dt-editing="true"]')).not.toBeNull();
  });

  it('marks a nullish cell with data-dt-null and leaves a valued one unmarked', () => {
    const { container } = render(<StoreGrid />);
    expect(cell(container, 1, 'name').getAttribute('data-dt-null')).toBe('true');
    expect(cell(container, 0, 'name').getAttribute('data-dt-null')).toBeNull();
  });

  it('exposes the read-only reason in data-dt-readonly', () => {
    const { container } = render(<StoreGrid />);
    expect(container.querySelector('[data-dt-readonly]')).toBeNull();
  });

  it('gives the header label and the cell of one column the same colour', () => {
    // MySQL's `tinyint(1)` is a boolean: the cell is edited with a checkbox,
    // so the header's type label must not be painted as a number (05 §3.1 —
    // one judgement, two projections).
    const mysql: CellTypeHints = {
      overrides: [{ match: 'exact', pattern: 'tinyint(1)', cellType: 'bool' }],
    };
    const columns = [{ name: 'flag', dataType: 'tinyint(1)', nullable: true }];
    const columnEditorInfo = buildColumnEditorInfo(columns, mysql);

    function HintedGrid() {
      return (
        <DataTable
          columns={[{ id: 'flag', name: 'flag', type: 'tinyint(1)' }]}
          rows={[[1]]}
          cellTypeHints={mysql}
          cellEditing={{ columnEditorInfo, readOnlyReason: null }}
          onCellEditCancel={() => undefined}
        />
      );
    }
    const { container } = render(<HintedGrid />);

    const headerLabel = container.querySelector('[title="tinyint(1)"]');
    const cell = container.querySelector('[data-dt-row="0"][data-dt-col="flag"]');
    const value = cell?.querySelector('span');
    expect(headerLabel?.className).toContain('text-dt-bool');
    expect(value?.className).toContain('text-dt-bool');
  });

  it('names the read-only reason of a read-only grid', () => {
    function ReadOnlyGrid() {
      const ts = useTableDataStore((s) => s.byPanel.get(PANEL));
      const columns = useMemo(
        () => (ts?.columns ?? []).map((c) => ({ id: c.name, name: c.name, type: c.dataType })),
        [ts],
      );
      const rows = useMemo(
        () => (ts?.rows ?? []).map((record) => columns.map((col) => record[col.name] ?? null)),
        [columns, ts],
      );
      const columnEditorInfo = useMemo(() => buildColumnEditorInfo(ts?.columns ?? []), [ts]);
      return (
        <DataTable
          columns={columns}
          rows={rows}
          primaryKeyColumns={['id']}
          editingCell={null}
          cellEditing={{ columnEditorInfo, readOnlyReason: 'view' }}
          onCellEditCancel={() => undefined}
        />
      );
    }
    const { container } = render(<ReadOnlyGrid />);
    const reasons = [...container.querySelectorAll('[data-dt-readonly]')].map((node) =>
      node.getAttribute('data-dt-readonly'),
    );
    expect(reasons.length).toBeGreaterThan(0);
    expect(new Set(reasons)).toEqual(new Set(['view']));
  });
});
