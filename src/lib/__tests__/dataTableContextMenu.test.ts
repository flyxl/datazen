import { describe, expect, it, vi } from 'vitest';
import {
  buildDataTableContextMenuItems,
  formatRowAsSqlInsert,
  formatRowAsSqlUpdate,
  formatRowAsSqlDelete,
  resolveDataTableCellFromEvent,
  resolveDataTableHeaderColFromEvent,
  rowToNamedRecord,
  serializeDataTableColumnValues,
  serializeDataTableRowsAsCsv,
  serializeDataTableRowsAsTsv,
  type DataTableContextMenuLabels,
} from '../dataTableContextMenu';

const labels: DataTableContextMenuLabels = {
  copy: 'Copy',
  copyRow: 'Copy Row',
  moreActions: 'More actions',
  copyAsJson: 'Copy as JSON',
  copyAsSqlInsert: 'Copy as SQL INSERT',
  copyAsUpdate: 'Copy as UPDATE',
  copyAsCsv: 'Copy as CSV',
  copyColumnName: 'Copy Column Name',
  copyColumnData: 'Copy Column Data',
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

function rootItemIds(items: ReturnType<typeof buildDataTableContextMenuItems>): string[] {
  return items.filter((i) => i.kind === 'item').map((i) => i.id);
}

function findSubmenu(
  items: ReturnType<typeof buildDataTableContextMenuItems>,
  id: string,
): Extract<(typeof items)[number], { kind: 'submenu' }> | undefined {
  return items.find(
    (i): i is Extract<(typeof items)[number], { kind: 'submenu' }> =>
      i.kind === 'submenu' && i.id === id,
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

describe('serializeDataTableColumnValues', () => {
  it('serializes column values separated by newlines, handling null and objects', () => {
    const rows = [
      [1, 'Alice', { role: 'admin' }],
      [2, null, { role: 'user' }],
      [3, 'Charlie', null],
    ];
    expect(serializeDataTableColumnValues(0, rows)).toBe('1\n2\n3');
    expect(serializeDataTableColumnValues(1, rows)).toBe('Alice\n\nCharlie');
    expect(serializeDataTableColumnValues(2, rows)).toBe('{"role":"admin"}\n{"role":"user"}\n');
  });

  it('returns empty string for negative column index', () => {
    expect(serializeDataTableColumnValues(-1, [[1]])).toBe('');
  });
});

describe('resolveDataTableHeaderColFromEvent', () => {
  it('returns column name from closest element with data-col-header', () => {
    const div = document.createElement('div');
    div.setAttribute('data-col-header', 'email');
    const child = document.createElement('span');
    div.appendChild(child);
    expect(resolveDataTableHeaderColFromEvent(child)).toBe('email');
    expect(resolveDataTableHeaderColFromEvent(div)).toBe('email');
    expect(resolveDataTableHeaderColFromEvent(null)).toBeNull();
  });
});

describe('rowToNamedRecord / formatRowAsSqlInsert / formatRowAsSqlUpdate', () => {
  it('maps columns to a JSON-friendly record', () => {
    expect(rowToNamedRecord(['id', 'name'], [1, 'Ada'])).toEqual({ id: 1, name: 'Ada' });
  });

  it('formats SQL INSERT with escaping (PostgreSQL)', () => {
    expect(formatRowAsSqlInsert('users', ['id', 'name'], [1, "O'Brien"], 'postgresql')).toBe(
      `INSERT INTO "users" ("id", "name") VALUES (1, 'O''Brien');`,
    );
  });

  it('formats SQL INSERT with escaping (MySQL)', () => {
    expect(formatRowAsSqlInsert('users', ['id', 'name'], [1, "O'Brien"], 'mysql')).toBe(
      "INSERT INTO `users` (`id`, `name`) VALUES (1, 'O''Brien');",
    );
  });

  it('formats SQL UPDATE using primary key or first column (PostgreSQL)', () => {
    expect(formatRowAsSqlUpdate('users', ['id', 'name'], [1, 'Ada'], ['id'], 'postgresql')).toBe(
      `UPDATE "users" SET "name" = 'Ada' WHERE "id" = 1;`,
    );
    expect(formatRowAsSqlUpdate('users', ['id', 'name'], [1, 'Ada'], undefined, 'postgresql')).toBe(
      `UPDATE "users" SET "name" = 'Ada' WHERE "id" = 1;`,
    );
  });

  it('formats SQL UPDATE using primary key or first column (MySQL)', () => {
    expect(formatRowAsSqlUpdate('users', ['id', 'name'], [1, 'Ada'], ['id'], 'mysql')).toBe(
      "UPDATE `users` SET `name` = 'Ada' WHERE `id` = 1;",
    );
  });

  it('formats SQL DELETE with PK WHERE and IS NULL for null keys', () => {
    expect(formatRowAsSqlDelete('users', ['id', 'name'], [1, 'Ada'], ['id'], 'postgresql')).toBe(
      `DELETE FROM "users" WHERE "id" = 1;`,
    );
    expect(formatRowAsSqlDelete('users', ['id', 'name'], [null, 'Ada'], ['id'], 'postgresql')).toBe(
      `DELETE FROM "users" WHERE "id" IS NULL;`,
    );
  });

  // --- JSON field escaping for PostgreSQL ---
  it('does NOT double-escape backslashes in JSON for PostgreSQL', () => {
    // PostgreSQL with standard_conforming_strings=on treats backslashes as literal.
    // JSON.stringify({ path: 'C:\\Users\\test' }) produces: {"path":"C:\\Users\\test"}
    // No extra escaping needed.
    const jsonVal = { path: 'C:\\Users\\test' };
    const result = formatRowAsSqlInsert('t', ['id', 'meta'], [1, jsonVal], 'postgresql');
    expect(result).toContain(`'{"path":"C:\\\\Users\\\\test"}'`);
  });

  it('escapes single quotes inside JSON object values (PostgreSQL)', () => {
    const jsonVal = { name: "O'Brien" };
    const result = formatRowAsSqlInsert('t', ['id', 'meta'], [1, jsonVal], 'postgresql');
    expect(result).toContain(`'{"name":"O''Brien"}'`);
  });

  it('escapes newline characters in JSON for PostgreSQL', () => {
    const jsonVal = { text: 'line1\nline2' };
    // PG standard_conforming_strings=on: backslash is literal
    const result = formatRowAsSqlInsert('t', ['id', 'meta'], [1, jsonVal], 'postgresql');
    expect(result).toContain(`'{"text":"line1\\nline2"}'`);
  });

  // --- JSON field escaping for MySQL ---
  // MySQL uses backslash as escape character, so we must:
  //   1. Escape \ → \\  (backslash)
  //   2. Escape " → \"  (double quote, for Navicat compatibility)
  //   3. Escape ' → ''  (single quote, SQL standard)

  it('escapes backslashes in JSON for MySQL compatibility', () => {
    const jsonVal = { path: 'C:\\Users\\test' };
    const result = formatRowAsSqlInsert('t', ['id', 'meta'], [1, jsonVal], 'mysql');
    // " are escaped as \", \ are escaped as \\
    expect(result).toContain(String.raw`'{\"path\":\"C:\\\\Users\\\\test\"}'`);
  });

  it('escapes backslash-escaped quotes in JSON for MySQL', () => {
    const jsonVal = { bio: 'He said "hello"' };
    const result = formatRowAsSqlInsert('t', ['id', 'meta'], [1, jsonVal], 'mysql');
    expect(result).toContain(String.raw`'{\"bio\":\"He said \\\"hello\\\"\"}'`);
  });

  it('escapes backslashes in JSON for MySQL UPDATE statements', () => {
    const jsonVal = { path: 'C:\\Users\\test' };
    const result = formatRowAsSqlUpdate('t', ['id', 'meta'], [1, jsonVal], ['id'], 'mysql');
    expect(result).toContain(String.raw`'{\"path\":\"C:\\\\Users\\\\test\"}'`);
  });

  it('escapes single quotes inside JSON object values (MySQL)', () => {
    const jsonVal = { name: "O'Brien" };
    const result = formatRowAsSqlInsert('t', ['id', 'meta'], [1, jsonVal], 'mysql');
    expect(result).toContain(String.raw`'{\"name\":\"O''Brien\"}'`);
  });

  it('escapes newline characters in JSON for MySQL', () => {
    const jsonVal = { text: 'line1\nline2' };
    const result = formatRowAsSqlInsert('t', ['id', 'meta'], [1, jsonVal], 'mysql');
    // \n in JSON.stringify output: \ is escaped to \\, so \n becomes \\n
    expect(result).toContain(String.raw`'{\"text\":\"line1\\nline2\"}'`);
  });

  it('handles deeply nested JSON with special characters', () => {
    const nestedJson = {
      user: {
        name: "O'Brien",
        bio: 'He said "hello"',
        metadata: { tags: ['admin', 'user\\role'] },
      },
    };
    const pgResult = formatRowAsSqlInsert('t', ['id', 'data'], [1, nestedJson], 'postgresql');
    expect(pgResult).toContain('INSERT INTO');
    expect(pgResult).toContain("'{");

    const myResult = formatRowAsSqlInsert('t', ['id', 'data'], [1, nestedJson], 'mysql');
    expect(myResult).toContain('INSERT INTO');
    expect(myResult).toContain("'{");
  });

  it('backward-compatible: no dialect defaults to PostgreSQL-style quoting', () => {
    expect(formatRowAsSqlInsert('users', ['id', 'name'], [1, 'Ada'])).toBe(
      `INSERT INTO "users" ("id", "name") VALUES (1, 'Ada');`,
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
  it('keeps frequent cell actions at root and groups the rest in More actions', () => {
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
      onDeleteRow: vi.fn(),
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
      canDelete: true,
    });
    expect(ids(items)).toEqual([
      'copy',
      'copy-row',
      'filter-by-value',
      'separator',
      'delete-row',
      'separator',
      'export',
      'separator',
      'submenu',
    ]);
    expect(rootItemIds(items)).toEqual([
      'copy',
      'copy-row',
      'filter-by-value',
      'delete-row',
      'export',
    ]);
    expect(rootItemIds(items)).not.toContain('set-null');

    const more = findSubmenu(items, 'more-actions');
    expect(more).toBeDefined();
    expect(more?.items.filter((i) => i.kind === 'item').map((i) => i.id)).toEqual([
      'copy-as-json',
      'copy-as-sql-insert',
      'copy-as-update',
      'copy-as-csv',
      'copy-column-name',
      'set-null',
      'copy-selected-rows',
    ]);

    for (const it of items) {
      if (it.kind === 'item') it.action();
    }
    for (const it of more?.items ?? []) {
      if (it.kind === 'item') it.action();
    }
    expect(handlers.onCopy).toHaveBeenCalledOnce();
    expect(handlers.onCopyRow).toHaveBeenCalledOnce();
    expect(handlers.onFilterByValue).toHaveBeenCalledOnce();
    expect(handlers.onSetNull).toHaveBeenCalledOnce();
    expect(handlers.onDeleteRow).toHaveBeenCalledOnce();
    expect(handlers.onExport).toHaveBeenCalledOnce();
    expect(handlers.onCopyAsJson).toHaveBeenCalledOnce();
    expect(handlers.onCopyAsSqlInsert).toHaveBeenCalledOnce();
    expect(handlers.onCopyAsUpdate).toHaveBeenCalledOnce();
    expect(handlers.onCopyAsCsv).toHaveBeenCalledOnce();
    expect(handlers.onCopyColumnName).toHaveBeenCalledOnce();
    expect(handlers.onCopySelectedRows).toHaveBeenCalledOnce();
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
    expect(rootItemIds(items)).toEqual(['copy', 'copy-row']);
    expect(findSubmenu(items, 'more-actions')?.items.map((i) => i.kind)).toEqual([
      'item',
      'item',
      'item',
      'item',
      'item',
    ]);
    expect(rootItemIds(items)).not.toContain('export');
    expect(rootItemIds(items)).not.toContain('set-null');
    expect(
      findSubmenu(items, 'more-actions')?.items.map((i) => i.kind === 'item' && i.id),
    ).not.toContain('set-null');
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
    expect(rootItemIds(items)).not.toContain('filter-by-value');
  });

  it('without cell context keeps delete/export at root and selection copies in submenu', () => {
    const items = buildDataTableContextMenuItems({
      labels,
      handlers: {
        onCopySelectedRows: vi.fn(),
        onCopyAsCsv: vi.fn(),
        onDeleteRow: vi.fn(),
        onExport: vi.fn(),
      },
      hasSelectedRows: true,
      canDelete: true,
      exportEnabled: true,
    });
    expect(rootItemIds(items)).toEqual(['delete-row', 'export']);
    expect(findSubmenu(items, 'more-actions')?.items.map((i) => i.kind === 'item' && i.id)).toEqual(
      ['copy-selected-rows', 'copy-as-csv'],
    );

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

  it('does not expose Filter by This Value while loading', () => {
    const items = buildDataTableContextMenuItems({
      labels,
      handlers: {
        onCopy: vi.fn(),
        onFilterByValue: vi.fn(),
      },
      hasCellContext: true,
      canFilterByValue: false,
    });
    expect(rootItemIds(items)).toEqual(['copy']);
    expect(rootItemIds(items)).not.toContain('filter-by-value');
  });

  it('with header context only, shows copy column name and copy column data at root', () => {
    const onCopyColumnName = vi.fn();
    const onCopyColumnData = vi.fn();
    const items = buildDataTableContextMenuItems({
      labels,
      handlers: {
        onCopyColumnName,
        onCopyColumnData,
      },
      hasHeaderContext: true,
      hasCellContext: false,
    });
    expect(rootItemIds(items)).toEqual(['copy-column-name', 'copy-column-data']);
    const colDataItem = items.find((i) => i.kind === 'item' && i.id === 'copy-column-data');
    if (colDataItem?.kind === 'item') colDataItem.action();
    expect(onCopyColumnData).toHaveBeenCalledOnce();
  });

  it('includes copy-column-data in more-actions when provided in cell context', () => {
    const onCopyColumnData = vi.fn();
    const items = buildDataTableContextMenuItems({
      labels,
      handlers: {
        onCopy: vi.fn(),
        onCopyColumnData,
      },
      hasCellContext: true,
    });
    const sub = findSubmenu(items, 'more-actions');
    expect(sub?.items.some((i) => i.kind === 'item' && i.id === 'copy-column-data')).toBe(true);
  });
});
