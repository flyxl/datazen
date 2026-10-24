import { describe, expect, it, vi } from 'vitest';
import {
  buildDataTableContextMenuItems,
  formatRowAsSqlInsert,
  formatRowAsSqlUpdate,
  formatRowAsSqlDelete,
  formatRowsAsSqlInsert,
  formatRowsAsSqlUpdate,
  resolveDataTableCellFromEvent,
  resolveDataTableHeaderColFromEvent,
  rowToNamedRecord,
  serializeDataTableColumnValues,
  serializeDataTableRowsAsCsv,
  serializeDataTableRowsAsJson,
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
  restoreRow: 'Restore This Row',
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

describe('serializeDataTableRowsAsJson', () => {
  it('serializes every row as an array of named records', () => {
    expect(
      serializeDataTableRowsAsJson(
        ['id', 'name'],
        [
          [1, 'Ada'],
          [2, null],
        ],
      ),
    ).toBe(
      JSON.stringify(
        [
          { id: 1, name: 'Ada' },
          { id: 2, name: null },
        ],
        null,
        2,
      ),
    );
  });
});

describe('formatRowsAsSqlInsert / formatRowsAsSqlUpdate', () => {
  it('emits one statement per row', () => {
    expect(
      formatRowsAsSqlInsert(
        'users',
        ['id', 'name'],
        [
          [1, 'Ada'],
          [2, 'Bob'],
        ],
        'postgresql',
      ),
    ).toBe(
      `INSERT INTO "users" ("id", "name") VALUES (1, 'Ada');\n` +
        `INSERT INTO "users" ("id", "name") VALUES (2, 'Bob');`,
    );
    expect(
      formatRowsAsSqlUpdate(
        'users',
        ['id', 'name'],
        [
          [1, 'Ada'],
          [2, 'Bob'],
        ],
        ['id'],
        'postgresql',
      ),
    ).toBe(
      `UPDATE "users" SET "name" = 'Ada' WHERE "id" = 1;\n` +
        `UPDATE "users" SET "name" = 'Bob' WHERE "id" = 2;`,
    );
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
    // DB-03 §5 Step 9: the column copy runs through the one TSV serializer, so a JSON value
    // is quoted and its inner quotes are doubled — otherwise a value containing a quote or a
    // line break would silently change the number of "rows" of the copy. The text is
    // byte-for-byte the same once a spreadsheet (or `parseClipboardMatrix`) reads it back.
    expect(serializeDataTableColumnValues(2, rows)).toBe(
      '"{""role"":""admin""}"\n"{""role"":""user""}"\n',
    );
  });

  it('returns empty string for negative column index', () => {
    expect(serializeDataTableColumnValues(-1, [[1]])).toBe('');
  });
});

describe('cell-selection clipboard block (DB-03)', () => {
  const clipboardLabels: DataTableContextMenuLabels = {
    ...labels,
    copySelectedCells: 'Copy Selected Cells',
    copySelectedCellsAs: 'Copy Selected Cells As…',
    copySelectedCellsAsTsv: 'Copy as TSV',
    copySelectedCellsAsTsvWithHeader: 'Copy as TSV with header',
    copySelectedCellsAsCsv: 'Copy as CSV',
    copySelectedCellsAsJson: 'Copy as JSON',
    copySelectedCellsAsPlain: 'Copy as Plain Text',
    copySelectedCellsAsHtml: 'Copy as HTML Table',
    copySelectedCellsAsMarkdown: 'Copy as Markdown Table',
    pasteSelectedCells: 'Paste to Selected Cells',
  };

  const baseHandlers = () => ({
    onCopy: vi.fn(),
    onCopyRow: vi.fn(),
    onCopySelectedCells: vi.fn(),
    onCopySelectedCellsAs: vi.fn(),
    onPasteSelectedCells: vi.fn(),
  });

  it('adds the block only when a region is reported', () => {
    // Regression: without the new argument the sequence is byte-identical to today's.
    const without = buildDataTableContextMenuItems({
      labels: clipboardLabels,
      handlers: baseHandlers(),
      hasCellContext: true,
    });
    expect(rootItemIds(without)).toEqual(['copy', 'copy-row']);

    const withRegion = buildDataTableContextMenuItems({
      labels: clipboardLabels,
      handlers: baseHandlers(),
      hasCellContext: true,
      hasCellRange: true,
      canCopySelection: true,
      canPasteSelection: true,
    });
    expect(rootItemIds(withRegion)).toEqual([
      'copy',
      'copy-row',
      'copy-selected-cells',
      'paste-selected-cells',
    ]);
  });

  it('routes every "copy as" format to the handler with its own options', () => {
    const handlers = baseHandlers();
    const items = buildDataTableContextMenuItems({
      labels: clipboardLabels,
      handlers,
      hasCellContext: true,
      hasCellRange: true,
      canCopySelection: true,
    });
    const copyAs = findSubmenu(items, 'copy-selected-cells-as');
    expect(copyAs?.label).toBe('Copy Selected Cells As…');

    const children = copyAs?.items ?? [];
    for (const child of children) {
      if (child.kind !== 'item') continue;
      child.action?.();
    }
    expect(handlers.onCopySelectedCellsAs.mock.calls).toEqual([
      ['tsv'],
      ['tsv', { includeHeader: true }],
      ['csv'],
      ['json'],
      // The DB-10 formats ride the same submenu after the established ones.
      ['plain'],
      ['html'],
      ['markdown'],
    ]);
  });

  it('offers the three DB-10 formats in the copy-as submenu', () => {
    const items = buildDataTableContextMenuItems({
      labels: clipboardLabels,
      handlers: baseHandlers(),
      hasCellContext: true,
      hasCellRange: true,
      canCopySelection: true,
    });
    const copyAs = findSubmenu(items, 'copy-selected-cells-as');
    const ids = (copyAs?.items ?? [])
      .filter((entry) => entry.kind === 'item')
      .map((entry) => (entry as { id: string }).id);
    expect(ids).toContain('copy-selected-cells-as-plain');
    expect(ids).toContain('copy-selected-cells-as-html');
    expect(ids).toContain('copy-selected-cells-as-markdown');
  });

  it('keeps an unavailable entry visible but disabled', () => {
    const items = buildDataTableContextMenuItems({
      labels: clipboardLabels,
      handlers: baseHandlers(),
      hasCellContext: true,
      hasCellRange: true,
      // A read-only connection: copying is fine, pasting is refused — and the user has to see
      // that the entry exists before wondering why nothing happens.
      canCopySelection: true,
      canPasteSelection: false,
    });
    const byId = new Map(
      items
        .filter(
          (entry): entry is Extract<(typeof items)[number], { kind: 'item' }> =>
            entry.kind === 'item',
        )
        .map((entry) => [entry.id, entry]),
    );
    // `NativeMenuItemDef` expresses "enabled" by **omitting** the flag, so the enabled item
    // must not carry `enabled: false` at all.
    expect(byId.get('copy-selected-cells')).toBeDefined();
    expect(byId.get('copy-selected-cells')?.enabled).not.toBe(false);
    expect(byId.get('paste-selected-cells')?.enabled).toBe(false);
  });

  it('says so when part of the selection cannot be copied', () => {
    const items = buildDataTableContextMenuItems({
      labels: clipboardLabels,
      handlers: baseHandlers(),
      hasCellContext: true,
      hasCellRange: true,
      canCopySelection: true,
      selectionIgnoredRanges: 2,
    });
    const copy = items.find(
      (entry): entry is Extract<(typeof items)[number], { kind: 'item' }> =>
        entry.kind === 'item' && entry.id === 'copy-selected-cells',
    );
    expect(copy?.label).toBe('Copy Selected Cells (2…)');
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
    expect(rootItemIds(items)).toEqual(['copy-selected-rows', 'delete-row', 'export']);
    expect(findSubmenu(items, 'more-actions')?.items.map((i) => i.kind === 'item' && i.id)).toEqual(
      ['copy-as-csv'],
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

  it('drops single-row actions when the menu targets a multi-row selection', () => {
    const handlers = {
      onCopy: vi.fn(),
      onCopyRow: vi.fn(),
      onCopyAsJson: vi.fn(),
      onCopyAsSqlInsert: vi.fn(),
      onCopyAsUpdate: vi.fn(),
      onCopyAsCsv: vi.fn(),
      onCopyColumnName: vi.fn(),
      onCopyColumnData: vi.fn(),
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
      multiRowSelection: true,
      hasSelectedRows: true,
      exportEnabled: true,
      canFilterByValue: true,
      canSetNull: true,
      canDelete: true,
    });

    // Cell/row-scoped actions cannot act on several rows, so they are gone.
    expect(rootItemIds(items)).toEqual([
      'copy-selected-rows',
      'filter-by-value',
      'delete-row',
      'export',
    ]);
    expect(rootItemIds(items)).not.toContain('copy');
    expect(rootItemIds(items)).not.toContain('copy-row');
    expect(rootItemIds(items)).not.toContain('set-null');

    const more = findSubmenu(items, 'more-actions');
    expect(more?.items.filter((i) => i.kind === 'item').map((i) => i.id)).toEqual([
      'copy-as-csv',
      'copy-as-json',
      'copy-as-sql-insert',
      'copy-as-update',
      'copy-column-name',
      'copy-column-data',
    ]);

    for (const it of items) {
      if (it.kind === 'item') it.action();
    }
    for (const it of more?.items ?? []) {
      if (it.kind === 'item') it.action();
    }
    expect(handlers.onCopy).not.toHaveBeenCalled();
    expect(handlers.onCopyRow).not.toHaveBeenCalled();
    expect(handlers.onSetNull).not.toHaveBeenCalled();
    expect(handlers.onCopySelectedRows).toHaveBeenCalledOnce();
    expect(handlers.onCopyAsJson).toHaveBeenCalledOnce();
    expect(handlers.onCopyAsSqlInsert).toHaveBeenCalledOnce();
    expect(handlers.onCopyAsUpdate).toHaveBeenCalledOnce();
    expect(handlers.onCopyAsCsv).toHaveBeenCalledOnce();
    expect(handlers.onFilterByValue).toHaveBeenCalledOnce();
    expect(handlers.onDeleteRow).toHaveBeenCalledOnce();
    expect(handlers.onExport).toHaveBeenCalledOnce();
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

describe('buildDataTableContextMenuItems header sort block (DB-11)', () => {
  const sortLabels: DataTableContextMenuLabels = {
    ...labels,
    sortMulti: 'Sort (multi-column)',
    sortAscending: 'Ascending',
    sortDescending: 'Descending',
    removeSortColumn: 'Remove from Sort',
    clearSorts: 'Clear All Sorting',
  };

  it('without headerSort, the header menu keeps its exact pre-DB-11 shape', () => {
    const items = buildDataTableContextMenuItems({
      labels: sortLabels,
      handlers: {
        onCopyColumnName: vi.fn(),
        onCopyColumnData: vi.fn(),
        onSortAscending: vi.fn(),
        onSortDescending: vi.fn(),
        onRemoveSort: vi.fn(),
        onClearSorts: vi.fn(),
      },
      hasHeaderContext: true,
      hasCellContext: false,
    });
    // The sort handlers exist, but no sorts context was offered: byte for
    // byte the two copy entries and nothing else.
    expect(ids(items)).toEqual(['copy-column-name', 'copy-column-data']);
  });

  it('offers the sort submenu and clear-all before the copy entries', () => {
    const onSortAscending = vi.fn();
    const onSortDescending = vi.fn();
    const onRemoveSort = vi.fn();
    const onClearSorts = vi.fn();
    const items = buildDataTableContextMenuItems({
      labels: sortLabels,
      handlers: {
        onCopyColumnName: vi.fn(),
        onCopyColumnData: vi.fn(),
        onSortAscending,
        onSortDescending,
        onRemoveSort,
        onClearSorts,
      },
      hasHeaderContext: true,
      hasCellContext: false,
      headerSort: {
        column: 'age',
        sorts: [
          { column: 'name', descending: false },
          { column: 'age', descending: true },
        ],
      },
    });
    expect(
      items.map((i) => (i.kind === 'item' || i.kind === 'submenu' ? i.id : 'separator')),
    ).toEqual(['sort-multi', 'clear-sorts', 'separator', 'copy-column-name', 'copy-column-data']);
    const sortMenu = findSubmenu(items, 'sort-multi');
    expect(
      sortMenu?.items.map((i) => (i.kind === 'item' || i.kind === 'submenu' ? i.id : i.kind)),
    ).toEqual(['sort-ascending', 'sort-descending', 'remove-sort-column']);
    // The right-clicked column is in the sort list, so removal is offered enabled.
    const removeItem = sortMenu?.items.find(
      (i): i is Extract<typeof i, { kind: 'item' }> =>
        i.kind === 'item' && i.id === 'remove-sort-column',
    );
    expect(removeItem?.enabled).toBeUndefined();
    // The actions are bound to the right-clicked column.
    for (const i of sortMenu?.items ?? []) {
      if (i.kind === 'item') i.action();
    }
    expect(onSortAscending).toHaveBeenCalledWith('age');
    expect(onSortDescending).toHaveBeenCalledWith('age');
    expect(onRemoveSort).toHaveBeenCalledWith('age');
    const clearItem = items.find(
      (i): i is Extract<typeof i, { kind: 'item' }> => i.kind === 'item' && i.id === 'clear-sorts',
    );
    clearItem?.action();
    expect(onClearSorts).toHaveBeenCalledOnce();
  });

  it('disables remove on a column that is not sorted and clear when no sorts exist', () => {
    const items = buildDataTableContextMenuItems({
      labels: sortLabels,
      handlers: {
        onCopyColumnName: vi.fn(),
        onCopyColumnData: vi.fn(),
        onSortAscending: vi.fn(),
        onSortDescending: vi.fn(),
        onRemoveSort: vi.fn(),
        onClearSorts: vi.fn(),
      },
      hasHeaderContext: true,
      hasCellContext: false,
      headerSort: { column: 'age', sorts: [] },
    });
    const sortMenu = findSubmenu(items, 'sort-multi');
    const removeItem = sortMenu?.items.find(
      (i): i is Extract<typeof i, { kind: 'item' }> =>
        i.kind === 'item' && i.id === 'remove-sort-column',
    );
    expect(removeItem?.enabled).toBe(false);
    const clearItem = items.find(
      (i): i is Extract<typeof i, { kind: 'item' }> => i.kind === 'item' && i.id === 'clear-sorts',
    );
    expect(clearItem?.enabled).toBe(false);
  });

  it('drops the whole sort block when the caller offers no sort handlers', () => {
    const items = buildDataTableContextMenuItems({
      labels: sortLabels,
      handlers: {
        onCopyColumnName: vi.fn(),
        onCopyColumnData: vi.fn(),
      },
      hasHeaderContext: true,
      hasCellContext: false,
      headerSort: { column: 'age', sorts: [{ column: 'name', descending: false }] },
    });
    // headerSort without handlers is inert: no submenu, no clear entry, no
    // separator — the header menu keeps its pre-DB-11 shape.
    expect(ids(items)).toEqual(['copy-column-name', 'copy-column-data']);
  });
});

describe('buildDataTableContextMenuItems freeze block (DB-12)', () => {
  const freezeLabels: DataTableContextMenuLabels = {
    ...labels,
    freezeThrough: 'Freeze through This Column',
    unfreezeColumns: 'Unfreeze Columns',
  };

  it('offers the freeze entries between the sort block and the copy entries', () => {
    const onFreezeThrough = vi.fn();
    const onUnfreezeColumns = vi.fn();
    const items = buildDataTableContextMenuItems({
      labels: freezeLabels,
      handlers: {
        onCopyColumnName: vi.fn(),
        onCopyColumnData: vi.fn(),
        onFreezeThrough,
        onUnfreezeColumns,
      },
      hasHeaderContext: true,
      hasCellContext: false,
      headerFreeze: { index: 1, frozen: 0 },
    });
    expect(
      items.map((i) => (i.kind === 'item' || i.kind === 'submenu' ? i.id : 'separator')),
    ).toEqual([
      'freeze-through',
      'unfreeze-columns',
      'separator',
      'copy-column-name',
      'copy-column-data',
    ]);
    const freezeItem = items.find(
      (i): i is Extract<typeof i, { kind: 'item' }> =>
        i.kind === 'item' && i.id === 'freeze-through',
    );
    freezeItem?.action();
    expect(onFreezeThrough).toHaveBeenCalledWith(1);
  });

  it('disables the no-op freeze and the freezeless unfreeze', () => {
    const items = buildDataTableContextMenuItems({
      labels: freezeLabels,
      handlers: {
        onCopyColumnName: vi.fn(),
        onCopyColumnData: vi.fn(),
        onFreezeThrough: vi.fn(),
        onUnfreezeColumns: vi.fn(),
      },
      hasHeaderContext: true,
      hasCellContext: false,
      // Freezing through column 1 when exactly one column is frozen is the
      // state in force — nothing to do; and there is nothing to unfreeze.
      headerFreeze: { index: 0, frozen: 1 },
    });
    const freezeItem = items.find(
      (i): i is Extract<typeof i, { kind: 'item' }> =>
        i.kind === 'item' && i.id === 'freeze-through',
    );
    expect(freezeItem?.enabled).toBe(false);
    const unfreezeItem = items.find(
      (i): i is Extract<typeof i, { kind: 'item' }> =>
        i.kind === 'item' && i.id === 'unfreeze-columns',
    );
    expect(unfreezeItem?.enabled).toBeUndefined();
  });

  it('without headerFreeze the header menu keeps its exact pre-DB-12 shape', () => {
    const items = buildDataTableContextMenuItems({
      labels: freezeLabels,
      handlers: {
        onCopyColumnName: vi.fn(),
        onCopyColumnData: vi.fn(),
        onFreezeThrough: vi.fn(),
        onUnfreezeColumns: vi.fn(),
      },
      hasHeaderContext: true,
      hasCellContext: false,
    });
    expect(ids(items)).toEqual(['copy-column-name', 'copy-column-data']);
  });
});

describe('buildDataTableContextMenuItems column stats entry (DB-12)', () => {
  const statsLabels: DataTableContextMenuLabels = {
    ...labels,
    columnStats: 'Column Stats',
  };

  it('offers the stats entry after the column-arrangement blocks', () => {
    const onColumnStats = vi.fn();
    const items = buildDataTableContextMenuItems({
      labels: statsLabels,
      handlers: {
        onCopyColumnName: vi.fn(),
        onCopyColumnData: vi.fn(),
        onFreezeThrough: vi.fn(),
        onUnfreezeColumns: vi.fn(),
        onColumnStats,
      },
      hasHeaderContext: true,
      hasCellContext: false,
      headerFreeze: { index: 0, frozen: 0 },
      headerStats: { column: 'price' },
    });
    expect(ids(items)).toEqual([
      'freeze-through',
      'unfreeze-columns',
      'separator',
      'column-stats',
      'copy-column-name',
      'copy-column-data',
    ]);
    const statsItem = items.find(
      (i): i is Extract<typeof i, { kind: 'item' }> => i.kind === 'item' && i.id === 'column-stats',
    );
    statsItem?.action();
    expect(onColumnStats).toHaveBeenCalledWith('price');
  });

  it('the stats entry carries the right-clicked column, not a position', () => {
    const onColumnStats = vi.fn();
    const items = buildDataTableContextMenuItems({
      labels: statsLabels,
      handlers: { onCopyColumnName: vi.fn(), onColumnStats },
      hasHeaderContext: true,
      hasCellContext: false,
      headerStats: { column: 'note' },
    });
    const statsItem = items.find(
      (i): i is Extract<typeof i, { kind: 'item' }> => i.kind === 'item' && i.id === 'column-stats',
    );
    statsItem?.action();
    expect(onColumnStats).toHaveBeenCalledWith('note');
  });

  it('without the handler the header menu keeps its exact pre-DB-12 shape', () => {
    // The host may still report the context; without a handler the entry is
    // not offered at all — a driver grid keeps its previous menu.
    const items = buildDataTableContextMenuItems({
      labels: statsLabels,
      handlers: { onCopyColumnName: vi.fn(), onCopyColumnData: vi.fn() },
      hasHeaderContext: true,
      hasCellContext: false,
      headerStats: { column: 'price' },
    });
    expect(ids(items)).toEqual(['copy-column-name', 'copy-column-data']);
  });

  it('a cell click never offers the column stats entry', () => {
    const items = buildDataTableContextMenuItems({
      labels: statsLabels,
      handlers: {
        onCopy: vi.fn(),
        onColumnStats: vi.fn(),
      },
      hasHeaderContext: false,
      hasCellContext: true,
      headerStats: { column: 'price' },
    });
    expect(rootItemIds(items)).not.toContain('column-stats');
  });
});
