import { describe, expect, it, vi } from 'vitest';
import {
  buildDataTableContextMenuItems,
  formatRowAsSqlInsert,
  resolveDataTableCellFromEvent,
  rowToNamedRecord,
  serializeDataTableRowsAsTsv,
  type DataTableContextMenuLabels,
} from '../dataTableContextMenu';

const labels: DataTableContextMenuLabels = {
  copy: 'Copy',
  copyRow: 'Copy Row',
  copyAsJson: 'Copy as JSON',
  copyAsSqlInsert: 'Copy as SQL INSERT',
  copyColumnName: 'Copy Column Name',
  filterByValue: 'Filter by This Value',
  copySelectedRows: 'Copy Selected Rows',
  export: 'Export',
};

function ids(items: ReturnType<typeof buildDataTableContextMenuItems>): string[] {
  return items.map((i) =>
    i.kind === 'item' ? i.id : i.kind === 'separator' ? 'separator' : i.kind,
  );
}

describe('serializeDataTableRowsAsTsv', () => {
  it('joins cells with tabs and rows with newlines; nullish becomes empty', () => {
    expect(
      serializeDataTableRowsAsTsv([
        [1, 'a', null],
        [2, undefined, 'b'],
      ]),
    ).toBe('1\ta\t\n2\t\tb');
  });
});

describe('rowToNamedRecord / formatRowAsSqlInsert', () => {
  it('maps columns to a JSON-friendly record', () => {
    expect(rowToNamedRecord(['id', 'name'], [1, 'Ada'])).toEqual({ id: 1, name: 'Ada' });
  });

  it('formats SQL INSERT with escaping', () => {
    expect(formatRowAsSqlInsert('users', ['id', 'name'], [1, "O'Brien"])).toBe(
      `INSERT INTO "users" ("id", "name") VALUES (1, 'O''Brien');`,
    );
  });
});

describe('resolveDataTableCellFromEvent', () => {
  it('reads data-dt-row/col from the event target chain', () => {
    const cell = document.createElement('div');
    cell.setAttribute('data-dt-row', '2');
    cell.setAttribute('data-dt-col', 'amount');
    const inner = document.createElement('span');
    cell.appendChild(inner);
    expect(resolveDataTableCellFromEvent(inner)).toEqual({ rowIndex: 2, columnName: 'amount' });
  });

  it('returns null when attributes are missing', () => {
    expect(resolveDataTableCellFromEvent(document.createElement('div'))).toBeNull();
    expect(resolveDataTableCellFromEvent(null)).toBeNull();
  });
});

describe('buildDataTableContextMenuItems', () => {
  it('builds a TablePlus-style multi-item menu with cell context', () => {
    const handlers = {
      onCopy: vi.fn(),
      onCopyRow: vi.fn(),
      onCopyAsJson: vi.fn(),
      onCopyAsSqlInsert: vi.fn(),
      onCopyColumnName: vi.fn(),
      onFilterByValue: vi.fn(),
      onCopySelectedRows: vi.fn(),
      onExport: vi.fn(),
    };
    const items = buildDataTableContextMenuItems({
      labels,
      handlers,
      hasCellContext: true,
      hasSelectedRows: true,
      exportEnabled: true,
      canFilterByValue: true,
    });
    expect(ids(items)).toEqual([
      'copy',
      'copy-row',
      'copy-as-json',
      'copy-as-sql-insert',
      'copy-column-name',
      'filter-by-value',
      'separator',
      'copy-selected-rows',
      'export',
    ]);
    for (const it of items) {
      if (it.kind === 'item') it.action();
    }
    expect(handlers.onCopy).toHaveBeenCalledOnce();
    expect(handlers.onExport).toHaveBeenCalledOnce();
  });

  it('keeps multiple copy actions even without export or selection', () => {
    const items = buildDataTableContextMenuItems({
      labels,
      handlers: {
        onCopy: vi.fn(),
        onCopyRow: vi.fn(),
        onCopyAsJson: vi.fn(),
        onCopyAsSqlInsert: vi.fn(),
        onCopyColumnName: vi.fn(),
      },
      hasCellContext: true,
    });
    expect(ids(items).length).toBeGreaterThanOrEqual(5);
    expect(ids(items)).not.toContain('export');
  });

  it('omits filter when canFilterByValue is false', () => {
    const items = buildDataTableContextMenuItems({
      labels,
      handlers: {
        onCopy: vi.fn(),
        onCopyRow: vi.fn(),
        onCopyAsJson: vi.fn(),
        onCopyAsSqlInsert: vi.fn(),
        onCopyColumnName: vi.fn(),
        onFilterByValue: vi.fn(),
        onExport: vi.fn(),
      },
      hasCellContext: true,
      exportEnabled: true,
      canFilterByValue: false,
    });
    expect(ids(items)).not.toContain('filter-by-value');
  });

  it('without cell context only offers selection copy and/or export', () => {
    expect(
      ids(
        buildDataTableContextMenuItems({
          labels,
          handlers: { onCopySelectedRows: vi.fn(), onExport: vi.fn() },
          hasSelectedRows: true,
          exportEnabled: true,
        }),
      ),
    ).toEqual(['copy-selected-rows', 'export']);

    expect(
      ids(
        buildDataTableContextMenuItems({
          labels,
          handlers: { onExport: vi.fn() },
          exportEnabled: true,
        }),
      ),
    ).toEqual(['export']);
  });
});
