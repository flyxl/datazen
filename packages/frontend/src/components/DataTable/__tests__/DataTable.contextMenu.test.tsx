/**
 * Menu equivalence guard for the extraction of the grid's right-click menu into
 * `useDataTableContextMenu.ts` (DB-01 GC-D4): only the location of the code moved.
 *
 * The item set, the item order, the submenu contents and the conditions under which an
 * item appears must stay byte-identical — an item that disappears because a condition was
 * dropped is a silent data-path regression (the user loses "copy as SQL INSERT" or
 * "delete row" without any error).
 */
import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { render, fireEvent, cleanup, waitFor } from '@testing-library/react';
import { DataTable } from '../DataTable';
import type { DirtyRowEntry } from '../../../stores/tableData/dirtyIndex';

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

const { showNativeContextMenu } = vi.hoisted(() => ({
  showNativeContextMenu: vi.fn().mockResolvedValue(undefined),
}));

vi.mock('../../../lib/nativeContextMenu', () => ({
  showNativeContextMenu: (...args: unknown[]) => showNativeContextMenu(...args),
}));

type MenuItem = { kind: string; id?: string; label?: string; items?: MenuItem[] };

const COLS = [
  { id: 'id', name: 'id', type: 'integer' },
  { id: 'name', name: 'name', type: 'varchar' },
];

const ROWS = [
  [1, 'Alice'],
  [2, 'Bob'],
  [3, 'Cara'],
];

interface MenuContext {
  /** Rows the grid considers selected. */
  selectedRows?: Set<number>;
  /** Where the right click lands. */
  target?: 'cell' | 'header' | 'blank';
  primaryKeyColumns?: string[];
  editable?: boolean;
  loading?: boolean;
  /** `null` = the grid gets no table name at all (export is then impossible). */
  exportTableName?: string | null;
  onAddFilter?: boolean;
  /** The dirty state of the page (DB-06): a row marked for deletion. */
  dirtyByRow?: ReadonlyMap<number, DirtyRowEntry> | null;
  /** The restore entry point (DB-06). */
  onRestoreRow?: (rowIndex: number) => void;
}

function renderGrid(context: MenuContext) {
  const {
    selectedRows = new Set<number>(),
    target = 'cell',
    primaryKeyColumns,
    editable = true,
    loading = false,
    exportTableName = 'users',
    onAddFilter = true,
    dirtyByRow = null,
    onRestoreRow,
  } = context;

  const { container } = render(
    <DataTable
      columns={COLS}
      rows={ROWS}
      selectedRows={selectedRows}
      loading={loading}
      primaryKeyColumns={primaryKeyColumns}
      exportTableName={exportTableName ?? undefined}
      onRowSelect={vi.fn()}
      onSelectAll={vi.fn()}
      onAddFilter={onAddFilter ? vi.fn() : undefined}
      onCellEdit={editable ? vi.fn() : undefined}
      enableSetNull={editable}
      onDeleteRows={vi.fn()}
      dirtyByRow={dirtyByRow}
      onRestoreRow={onRestoreRow}
    />,
  );

  const hit =
    target === 'cell'
      ? container.querySelector('[data-dt-row="0"][data-dt-col="name"]')!
      : target === 'header'
        ? container.querySelector('[data-col-header="id"]')!
        : container.querySelector('.overflow-auto')!;

  fireEvent.contextMenu(hit, { clientX: 10, clientY: 10 });
  return container;
}

async function menuItems(): Promise<MenuItem[]> {
  await waitFor(() => expect(showNativeContextMenu).toHaveBeenCalled());
  return showNativeContextMenu.mock.calls[0]![0] as MenuItem[];
}

function rootIds(items: MenuItem[]): string[] {
  return items.filter((item) => item.kind === 'item').map((item) => item.id ?? '');
}

function submenuIds(items: MenuItem[]): string[] {
  const more = items.find((item) => item.kind === 'submenu');
  return (more?.items ?? []).filter((item) => item.kind === 'item').map((item) => item.id ?? '');
}

beforeEach(() => {
  vi.clearAllMocks();
});

afterEach(cleanup);

describe('DataTable context menu equivalence', () => {
  it('offers the cell menu with every frequent item and the full more-actions submenu', async () => {
    renderGrid({ selectedRows: new Set([0]) });
    const items = await menuItems();

    expect(rootIds(items)).toEqual(['copy', 'copy-row', 'filter-by-value', 'export']);
    expect(submenuIds(items)).toEqual([
      'copy-as-json',
      'copy-as-sql-insert',
      'copy-as-update',
      'copy-as-csv',
      'copy-column-name',
      'copy-column-data',
      'set-null',
      'copy-selected-rows',
    ]);
  });

  it('adds delete-row only when primary keys are present', async () => {
    renderGrid({ selectedRows: new Set([0]), primaryKeyColumns: ['id'] });
    const items = await menuItems();

    expect(rootIds(items)).toEqual(['copy', 'copy-row', 'filter-by-value', 'delete-row', 'export']);
    // The label carries the number of rows the action will hit.
    const deleteItem = items.find((item) => item.id === 'delete-row');
    expect(deleteItem?.label).toBe('dataTable.deleteRow');
  });

  it('offers restore-row on a row that is marked for deletion, and only there', async () => {
    const deletedRow: DirtyRowEntry = {
      identityKey: '["id":1]',
      state: 'deleted',
      changedColumns: [],
      cells: [],
    };
    const dirtyRow: DirtyRowEntry = {
      identityKey: '["id":2]',
      state: 'dirty',
      changedColumns: ['name'],
      cells: [{ columnName: 'name', original: 'Bob', next: 'Bobby' }],
    };
    const dirtyByRow = new Map<number, DirtyRowEntry>([
      [0, deletedRow],
      [1, dirtyRow],
    ]);

    // The right click lands on row 0 — the row that is marked for deletion.
    renderGrid({ dirtyByRow, onRestoreRow: vi.fn() });
    const items = await menuItems();
    expect(rootIds(items)).toContain('restore-row');
    const restoreItem = items.find((item) => item.id === 'restore-row');
    expect(restoreItem?.label).toBe('tableData.restoreRow');

    // A grid without the restore entry point does not offer the item.
    vi.clearAllMocks();
    renderGrid({ dirtyByRow });
    const withoutHandler = await menuItems();
    expect(rootIds(withoutHandler)).not.toContain('restore-row');
  });

  it('does not offer restore-row on a row that only has staged cell changes', async () => {
    const dirtyRow: DirtyRowEntry = {
      identityKey: '["id":1]',
      state: 'dirty',
      changedColumns: ['name'],
      cells: [{ columnName: 'name', original: 'Alice', next: 'Alicia' }],
    };
    renderGrid({
      dirtyByRow: new Map<number, DirtyRowEntry>([[0, dirtyRow]]),
      onRestoreRow: vi.fn(),
    });
    const items = await menuItems();
    expect(rootIds(items)).not.toContain('restore-row');
  });

  it('drops set-null on a read-only grid', async () => {
    renderGrid({ selectedRows: new Set([0]), editable: false });
    const items = await menuItems();

    expect(submenuIds(items)).not.toContain('set-null');
    expect(submenuIds(items)).toContain('copy-selected-rows');
  });

  it('drops filter-by-value while loading', async () => {
    renderGrid({ selectedRows: new Set([0]), loading: true });
    const items = await menuItems();

    expect(rootIds(items)).not.toContain('filter-by-value');
  });

  it('drops filter-by-value when the grid cannot add filters', async () => {
    renderGrid({ selectedRows: new Set([0]), onAddFilter: false });
    const items = await menuItems();

    expect(rootIds(items)).not.toContain('filter-by-value');
  });

  it('switches to the selection menu inside a multi-row selection', async () => {
    renderGrid({ selectedRows: new Set([0, 1]), primaryKeyColumns: ['id'] });
    const items = await menuItems();

    expect(rootIds(items)).toEqual([
      'copy-selected-rows',
      'filter-by-value',
      'delete-row',
      'export',
    ]);
    expect(submenuIds(items)).toEqual([
      'copy-as-csv',
      'copy-as-json',
      'copy-as-sql-insert',
      'copy-as-update',
      'copy-column-name',
      'copy-column-data',
    ]);
    // Single-cell / single-row actions are deliberately gone in selection mode.
    expect(rootIds(items)).not.toContain('copy');
    expect(rootIds(items)).not.toContain('copy-row');
    expect(submenuIds(items)).not.toContain('set-null');
  });

  it('offers the selection menu on empty space while rows are selected', async () => {
    renderGrid({ selectedRows: new Set([0]), target: 'blank' });
    const items = await menuItems();

    // No cell and no column header was hit, so the column copy items are not offered.
    expect(rootIds(items)).toEqual(['copy-selected-rows', 'export']);
    expect(submenuIds(items)).toEqual([
      'copy-as-csv',
      'copy-as-json',
      'copy-as-sql-insert',
      'copy-as-update',
    ]);
  });

  it('offers the header menu on a column header', async () => {
    renderGrid({ selectedRows: new Set([0]), target: 'header', exportTableName: 'users' });
    const items = await menuItems();

    expect(rootIds(items)).toEqual(['copy-column-name', 'copy-column-data', 'export']);
    expect(submenuIds(items)).toEqual([]);
  });

  it('hides the export item when the grid cannot export', async () => {
    renderGrid({ selectedRows: new Set([0]), exportTableName: null });
    const items = await menuItems();

    expect(rootIds(items)).not.toContain('export');
  });
});
