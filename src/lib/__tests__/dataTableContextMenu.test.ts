import { describe, expect, it, vi } from 'vitest';
import {
  buildDataTableContextMenuItems,
  formatRowAsSqlInsert,
  formatRowAsSqlUpdate,
  formatRowAsSqlDelete,
  resolveDataTableCellFromEvent,
  rowToNamedRecord,
  serializeDataTableRowsAsCsv,
  serializeDataTableRowsAsTsv,
  type DataTableContextMenuLabels,
} from '../dataTableContextMenu';

const labels: DataTableContextMenuLabels = {
  copy: 'Copy',
  copyRow: 'Copy Row',
  copyAsJson: 'Copy as JSON',
  copyAsSqlInsert: 'Copy as SQL INSERT',
  copyAsUpdate: 'Copy as UPDATE',
  copyAsCsv: 'Copy as CSV',
  copyColumnName: 'Copy Column Name',
  setNull: 'Set NULL',
  filterByValue: 'Filter by This Value',
  copySelectedRows: 'Copy Selected Rows',
  deleteRow: 'Delete Row',
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

describe('serializeDataTableRowsAsCsv', () => {
  it('includes header and escapes commas/quotes', () => {
    expect(
      serializeDataTableRowsAsCsv(
        ['id', 'name'],
        [
          [1, 'Ada'],
          [2, 'O"Brien, Jr'],
        ],
      ),
    ).toBe('id,name\n1,Ada\n2,"O""Brien, Jr"');
  });
});

describe('rowToNamedRecord / formatRowAsSqlInsert / formatRowAsSqlUpdate', () => {
  it('maps columns to a JSON-friendly record', () => {
    expect(rowToNamedRecord(['id', 'name'], [1, 'Ada'])).toEqual({ id: 1, name: 'Ada' });
  });

  it('formats SQL INSERT with escaping', () => {
    expect(formatRowAsSqlInsert('users', ['id', 'name'], [1, "O'Brien"])).toBe(
      `INSERT INTO "users" ("id", "name") VALUES (1, 'O''Brien');`,
    );
  });

  it('formats SQL UPDATE using primary key or first column', () => {
    expect(formatRowAsSqlUpdate('users', ['id', 'name'], [1, 'Ada'], ['id'])).toBe(
      `UPDATE "users" SET "name" = 'Ada' WHERE "id" = 1;`,
    );
    expect(formatRowAsSqlUpdate('users', ['id', 'name'], [1, 'Ada'])).toBe(
      `UPDATE "users" SET "name" = 'Ada' WHERE "id" = 1;`,
    );
  });

  it('formats SQL DELETE with PK WHERE and IS NULL for null keys', () => {
    expect(formatRowAsSqlDelete('users', ['id', 'name'], [1, 'Ada'], ['id'])).toBe(
      `DELETE FROM "users" WHERE "id" = 1;`,
    );
    expect(formatRowAsSqlDelete('users', ['id', 'name'], [null, 'Ada'], ['id'])).toBe(
      `DELETE FROM "users" WHERE "id" IS NULL;`,
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
      onCopyAsUpdate: vi.fn(),
      onCopyAsCsv: vi.fn(),
      onCopyColumnName: vi.fn(),
      onSetNull: vi.fn(),
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
      canSetNull: true,
    });
    expect(ids(items)).toEqual([
      'copy',
      'copy-row',
      'copy-as-json',
      'copy-as-sql-insert',
      'copy-as-update',
      'copy-as-csv',
      'copy-column-name',
      'set-null',
      'filter-by-value',
      'separator',
      'copy-selected-rows',
      'export',
    ]);
    for (const it of items) {
      if (it.kind === 'item') it.action();
    }
    expect(handlers.onCopy).toHaveBeenCalledOnce();
    expect(handlers.onSetNull).toHaveBeenCalledOnce();
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
        onCopyAsUpdate: vi.fn(),
        onCopyAsCsv: vi.fn(),
        onCopyColumnName: vi.fn(),
      },
      hasCellContext: true,
    });
    expect(ids(items).length).toBeGreaterThanOrEqual(5);
    expect(ids(items)).not.toContain('export');
    expect(ids(items)).not.toContain('set-null');
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

  it('without cell context only offers selection copy / csv and/or export', () => {
    expect(
      ids(
        buildDataTableContextMenuItems({
          labels,
          handlers: {
            onCopySelectedRows: vi.fn(),
            onCopyAsCsv: vi.fn(),
            onExport: vi.fn(),
          },
          hasSelectedRows: true,
          exportEnabled: true,
        }),
      ),
    ).toEqual(['copy-selected-rows', 'copy-as-csv', 'export']);

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

  it('includes delete-row when canDelete', () => {
    const onDeleteRow = vi.fn();
    const withCell = buildDataTableContextMenuItems({
      labels,
      handlers: { onCopy: vi.fn(), onDeleteRow },
      hasCellContext: true,
      canDelete: true,
    });
    expect(ids(withCell)).toContain('delete-row');
    const del = withCell.find((i) => i.kind === 'item' && i.id === 'delete-row');
    if (del?.kind === 'item') del.action();
    expect(onDeleteRow).toHaveBeenCalledOnce();

    expect(
      ids(
        buildDataTableContextMenuItems({
          labels,
          handlers: { onCopySelectedRows: vi.fn(), onDeleteRow },
          hasSelectedRows: true,
          canDelete: true,
        }),
      ),
    ).toContain('delete-row');
  });
});
