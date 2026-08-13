import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  batchExportNeedsZip,
  buildBatchExportFiles,
  combineBatchExportFiles,
  getBatchExportDefaultFilename,
  selectTablesForExport,
  type BatchExportTableInput,
} from '../batchExport';
import * as exportData from '../exportData';

const users: BatchExportTableInput = {
  tableName: 'users',
  ddl: 'CREATE TABLE users (id INT PRIMARY KEY, name TEXT);',
  columns: [{ name: 'id' }, { name: 'name' }],
  rows: [
    { id: 1, name: 'Alice' },
    { id: 2, name: 'Bob' },
  ],
};

const orders: BatchExportTableInput = {
  tableName: 'orders',
  ddl: 'CREATE TABLE orders (id INT);',
  columns: [{ name: 'id' }],
  rows: [{ id: 10 }],
};

describe('selectTablesForExport', () => {
  const all = ['users', 'orders', 'products'];

  it('returns all names when selected is "all"', () => {
    expect(selectTablesForExport(all, 'all')).toEqual(['users', 'orders', 'products']);
  });

  it('returns a copy of allTableNames for "all" (not the same reference)', () => {
    const result = selectTablesForExport(all, 'all');
    expect(result).not.toBe(all);
  });

  it('keeps selected names that exist in all, in selection order', () => {
    expect(selectTablesForExport(all, ['products', 'users'])).toEqual(['products', 'users']);
  });

  it('dedupes selected names', () => {
    expect(selectTablesForExport(all, ['users', 'users', 'orders'])).toEqual(['users', 'orders']);
  });

  it('drops names not present in all', () => {
    expect(selectTablesForExport(all, ['users', 'missing', 'orders'])).toEqual(['users', 'orders']);
  });

  it('returns empty array for empty selection', () => {
    expect(selectTablesForExport(all, [])).toEqual([]);
  });
});

describe('buildBatchExportFiles structure_only', () => {
  it('emits one .sql file per table with DDL', () => {
    const files = buildBatchExportFiles({
      tables: [users, orders],
      mode: 'structure_only',
      dataFormat: 'csv',
    });
    expect(files).toEqual([
      { filename: 'users.sql', content: users.ddl },
      { filename: 'orders.sql', content: orders.ddl },
    ]);
  });

  it('writes unavailable comment when ddl is missing', () => {
    const files = buildBatchExportFiles({
      tables: [
        { ...users, ddl: null },
        { ...orders, ddl: undefined },
        { ...orders, tableName: 'empty', ddl: '  ' },
      ],
      mode: 'structure_only',
      dataFormat: 'json',
    });
    expect(files[0].content).toBe('-- DDL unavailable for users');
    expect(files[1].content).toBe('-- DDL unavailable for orders');
    expect(files[2].content).toBe('-- DDL unavailable for empty');
  });
});

describe('buildBatchExportFiles data_only', () => {
  it('exports csv via generateExport', () => {
    const files = buildBatchExportFiles({
      tables: [users],
      mode: 'data_only',
      dataFormat: 'csv',
    });
    expect(files).toHaveLength(1);
    expect(files[0].filename).toBe('users.csv');
    expect(files[0].content).toBe('id,name\n1,Alice\n2,Bob');
  });

  it('exports json via generateExport', () => {
    const files = buildBatchExportFiles({
      tables: [users],
      mode: 'data_only',
      dataFormat: 'json',
    });
    expect(files[0].filename).toBe('users.json');
    expect(JSON.parse(files[0].content)).toEqual([
      { id: 1, name: 'Alice' },
      { id: 2, name: 'Bob' },
    ]);
  });

  it('exports sql_insert with databaseType quoting', () => {
    const files = buildBatchExportFiles({
      tables: [users],
      mode: 'data_only',
      dataFormat: 'sql_insert',
      databaseType: 'postgresql',
    });
    expect(files[0].filename).toBe('users.sql');
    expect(files[0].content).toContain('INSERT INTO "users"');
    expect(files[0].content).toContain('"id", "name"');
  });

  it('accepts ColumnSchema-shaped columns', () => {
    const files = buildBatchExportFiles({
      tables: [
        {
          tableName: 't',
          columns: [
            {
              name: 'id',
              dataType: 'int',
              nullable: false,
              isPrimaryKey: true,
              isAutoIncrement: true,
            },
          ],
          rows: [{ id: 1 }],
        },
      ],
      mode: 'data_only',
      dataFormat: 'csv',
    });
    expect(files[0].content).toBe('id\n1');
  });

  it('throws when generateExport returns non-text', () => {
    const spy = vi.spyOn(exportData, 'generateExport').mockReturnValue({
      kind: 'binary',
      dataBase64: 'AA==',
      extension: 'xlsx',
      mimeType: 'application/octet-stream',
    });
    expect(() =>
      buildBatchExportFiles({
        tables: [users],
        mode: 'data_only',
        dataFormat: 'csv',
      }),
    ).toThrow(/must produce text/);
    spy.mockRestore();
  });
});

describe('buildBatchExportFiles data_and_structure', () => {
  it('merges DDL + INSERT into one .sql when dataFormat is sql_insert', () => {
    const files = buildBatchExportFiles({
      tables: [users],
      mode: 'data_and_structure',
      dataFormat: 'sql_insert',
      databaseType: 'postgresql',
    });
    expect(files).toHaveLength(1);
    expect(files[0].filename).toBe('users.sql');
    expect(files[0].content.startsWith(users.ddl!)).toBe(true);
    expect(files[0].content).toContain('INSERT INTO "users"');
  });

  it('uses unavailable DDL comment still paired with inserts', () => {
    const files = buildBatchExportFiles({
      tables: [{ ...users, ddl: null }],
      mode: 'data_and_structure',
      dataFormat: 'sql_insert',
    });
    expect(files[0].content.startsWith('-- DDL unavailable for users\n\n')).toBe(true);
    expect(files[0].content).toContain('INSERT INTO');
  });

  it('emits separate .sql + .csv files', () => {
    const files = buildBatchExportFiles({
      tables: [users],
      mode: 'data_and_structure',
      dataFormat: 'csv',
    });
    expect(files).toEqual([
      { filename: 'users.sql', content: users.ddl },
      { filename: 'users.csv', content: 'id,name\n1,Alice\n2,Bob' },
    ]);
  });

  it('emits separate .sql + .json files', () => {
    const files = buildBatchExportFiles({
      tables: [orders],
      mode: 'data_and_structure',
      dataFormat: 'json',
    });
    expect(files.map((f) => f.filename)).toEqual(['orders.sql', 'orders.json']);
    expect(files[0].content).toBe(orders.ddl);
    expect(JSON.parse(files[1].content)).toEqual([{ id: 10 }]);
  });

  it('still emits data file when ddl is missing for csv/json', () => {
    const files = buildBatchExportFiles({
      tables: [{ ...users, ddl: null }],
      mode: 'data_and_structure',
      dataFormat: 'csv',
    });
    expect(files).toHaveLength(2);
    expect(files[0]).toEqual({
      filename: 'users.sql',
      content: '-- DDL unavailable for users',
    });
    expect(files[1].filename).toBe('users.csv');
    expect(files[1].content).toContain('Alice');
  });

  it('returns empty array for empty tables', () => {
    expect(
      buildBatchExportFiles({
        tables: [],
        mode: 'data_and_structure',
        dataFormat: 'csv',
      }),
    ).toEqual([]);
  });
});

describe('combineBatchExportFiles', () => {
  it('returns empty string for no files', () => {
    expect(combineBatchExportFiles([])).toBe('');
  });

  it('joins with default filename banners', () => {
    const combined = combineBatchExportFiles([
      { filename: 'users.sql', content: 'CREATE TABLE users;' },
      { filename: 'orders.sql', content: 'CREATE TABLE orders;' },
    ]);
    expect(combined).toBe(
      [
        '-- ===== users.sql =====',
        '',
        'CREATE TABLE users;',
        '',
        '-- ===== orders.sql =====',
        '',
        'CREATE TABLE orders;',
      ].join('\n'),
    );
  });

  it('uses custom separator when provided', () => {
    const combined = combineBatchExportFiles(
      [
        { filename: 'a.csv', content: 'a' },
        { filename: 'b.csv', content: 'b' },
      ],
      '\n---\n',
    );
    expect(combined).toBe('a\n---\nb');
  });
});

describe('batchExportNeedsZip', () => {
  it('zips mixed sql+csv even when output is single', () => {
    expect(
      batchExportNeedsZip(
        [
          { filename: 'users.sql', content: 'CREATE' },
          { filename: 'users.csv', content: 'id\n1' },
        ],
        'single',
      ),
    ).toBe(true);
  });

  it('zips multiple csv files', () => {
    expect(
      batchExportNeedsZip(
        [
          { filename: 'a.csv', content: 'a' },
          { filename: 'b.csv', content: 'b' },
        ],
        'single',
      ),
    ).toBe(true);
  });

  it('keeps a single json file as text', () => {
    expect(batchExportNeedsZip([{ filename: 'users.json', content: '[]' }], 'single')).toBe(false);
  });
});

describe('getBatchExportDefaultFilename', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-08-13T01:02:03.000Z'));
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('names structure export', () => {
    expect(getBatchExportDefaultFilename('structure_only', false)).toBe(
      'export_structure_2026-08-13-01-02-03.sql',
    );
  });

  it('names data export with format extension', () => {
    expect(getBatchExportDefaultFilename('data_only', false, 'json')).toBe(
      'export_data_2026-08-13-01-02-03.json',
    );
    expect(getBatchExportDefaultFilename('data_only', false, 'csv')).toBe(
      'export_data_2026-08-13-01-02-03.csv',
    );
  });

  it('names zip when zip flag is set', () => {
    expect(getBatchExportDefaultFilename('data_and_structure', true, 'csv')).toBe(
      'export_full_2026-08-13-01-02-03.zip',
    );
  });
});
