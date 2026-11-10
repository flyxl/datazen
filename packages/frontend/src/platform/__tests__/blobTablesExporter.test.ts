import { describe, expect, it, vi } from 'vitest';
import * as XLSX from 'xlsx';

import { createBlobTablesExporter, downloadBlob } from '../blobTablesExporter';
import type { TablesExportRequest } from '../contracts';
import type { Value } from '../../types';

/**
 * The F4 dual-flow demonstration: the web-shaped exporter answers the same
 * `exportTables(request)` signature the desktop Rust-streaming one does —
 * pages in through an injected reader, CSV out through a Blob download.
 */

const REQUEST = (overrides?: Partial<TablesExportRequest>): TablesExportRequest => ({
  dbSessionId: 'sess-1',
  mode: 'data_only',
  dataFormat: 'csv',
  outputMode: 'single',
  tables: [{ tableName: 'users', columns: ['id', 'name'] }],
  ...overrides,
});

const PINNED_NOW = () => new Date('2026-11-09T05:00:00.000Z');

describe('createBlobTablesExporter', () => {
  it('assembles paged reads into CSV and delivers one blob', async () => {
    const delivered: Array<{ contents: string; fileName: string }> = [];
    const readPage = vi
      .fn<(table: string, cursor: unknown) => Promise<import('../blobTablesExporter').TablePage>>()
      .mockResolvedValueOnce({
        columns: ['id', 'name'],
        rows: [
          [1, 'ada'],
          [2, 'grace'],
        ],
        nextCursor: 'page-2',
      })
      .mockResolvedValueOnce({
        columns: ['id', 'name'],
        rows: [[3, 'linus']],
        nextCursor: null,
      });

    const exporter = createBlobTablesExporter(readPage, {
      deliver: (contents, fileName) => delivered.push({ contents: contents as string, fileName }),
      now: PINNED_NOW,
    });

    await expect(exporter.exportTables(REQUEST())).resolves.toEqual({ Saved: 1 });

    expect(readPage).toHaveBeenCalledTimes(2);
    // WR-E3: 每次页读都携带请求域读目标（reader 看不到 request，由导出器过桥）。
    const context = { dbSessionId: 'sess-1', database: null, schema: null, driverType: null };
    expect(readPage).toHaveBeenNthCalledWith(1, 'users', null, context);
    expect(readPage).toHaveBeenNthCalledWith(2, 'users', 'page-2', context);
    expect(delivered).toHaveLength(1);
    // Item 5: the download name follows the shared default-name rule
    // (desktop `default_export_name`), not the old `{first}.csv`.
    expect(delivered[0]?.fileName).toBe('export_data_2026-11-09-05-00-00.csv');
    expect(delivered[0]?.contents).toContain('id,name');
    expect(delivered[0]?.contents).toContain('1,ada');
    expect(delivered[0]?.contents).toContain('3,linus');
    expect(delivered[0]?.contents).not.toContain('page-2');
  });

  it('quotes cells that could re-enter as two fields or a formula', async () => {
    const readPage = vi
      .fn<(table: string, cursor: unknown) => Promise<import('../blobTablesExporter').TablePage>>()
      .mockResolvedValue({
        columns: ['note'],
        rows: [['a,b'], ['say "hi"'], ['=SUM(A1)'], ['plain']],
        nextCursor: null,
      });
    const delivered: string[] = [];
    const exporter = createBlobTablesExporter(readPage, {
      deliver: (contents) => delivered.push(contents as string),
    });

    await exporter.exportTables(REQUEST());

    expect(delivered[0]).toContain('"a,b"');
    expect(delivered[0]).toContain('"say ""hi"""');
    expect(delivered[0]).toContain('"=SUM(A1)"');
    expect(delivered[0]).toContain('plain');
  });

  it('emits a provided source as CSV without re-reading the table (item 3c)', async () => {
    const readPage =
      vi.fn<
        (table: string, cursor: unknown) => Promise<import('../blobTablesExporter').TablePage>
      >();
    const delivered: string[] = [];
    const exporter = createBlobTablesExporter(readPage, {
      deliver: (contents) => delivered.push(contents as string),
    });

    await expect(
      exporter.exportTables(
        REQUEST({
          tables: [
            {
              tableName: 'users',
              columns: ['id', 'name'],
              source: {
                type: 'provided',
                columns: [
                  { name: 'id', dataType: 'int', nullable: false },
                  { name: 'name', dataType: 'text', nullable: true },
                ],
                rows: [
                  [1, 'a,b'],
                  [2, null],
                ],
              },
            },
          ],
        }),
      ),
    ).resolves.toEqual({ Saved: 1 });

    expect(readPage).not.toHaveBeenCalled();
    expect(delivered[0]).toContain('id,name');
    expect(delivered[0]).toContain('1,"a,b"');
    expect(delivered[0]).toContain('2,');
  });

  it('refuses a sql source instead of silently exporting the whole table (item 3c)', async () => {
    const readPage =
      vi.fn<
        (table: string, cursor: unknown) => Promise<import('../blobTablesExporter').TablePage>
      >();
    const exporter = createBlobTablesExporter(readPage);

    await expect(
      exporter.exportTables(
        REQUEST({
          tables: [
            {
              tableName: 'users',
              columns: ['id'],
              source: { type: 'sql', sql: 'SELECT id FROM users WHERE id = 1' },
            },
          ],
        }),
      ),
    ).rejects.toThrow('filtered export is not supported by the web exporter');
    expect(readPage).not.toHaveBeenCalled();
  });

  it('structure modes emit the DDL as a leading comment', async () => {
    const readPage =
      vi.fn<
        (table: string, cursor: unknown) => Promise<import('../blobTablesExporter').TablePage>
      >();
    const delivered: string[] = [];
    const exporter = createBlobTablesExporter(readPage, {
      loadDdl: async () => 'CREATE TABLE users (id INT);',
      deliver: (contents) => delivered.push(contents as string),
    });

    await exporter.exportTables(REQUEST({ mode: 'structure_only' }));

    expect(readPage).not.toHaveBeenCalled();
    expect(delivered[0]).toContain('CREATE TABLE users (id INT);');
  });
});

describe('web/desktop alignment (item 5)', () => {
  const capture = () => {
    const delivered: Array<{ contents: string; fileName: string; mimeType?: string }> = [];
    const readPage = vi
      .fn<(table: string, cursor: unknown) => Promise<import('../blobTablesExporter').TablePage>>()
      .mockResolvedValue({ columns: ['id'], rows: [[1]], nextCursor: null });
    return { delivered, readPage };
  };
  const exporterWith = (
    readPage: ReturnType<typeof capture>['readPage'],
    delivered: Array<{ contents: string; fileName: string; mimeType?: string }>,
    loadDdl?: (tableName: string) => Promise<string | null>,
  ) =>
    createBlobTablesExporter(readPage, {
      loadDdl,
      deliver: (contents, fileName, mimeType) =>
        delivered.push({ contents: contents as string, fileName, mimeType }),
      now: PINNED_NOW,
    });

  it.each(['tsv', 'json', 'markdown', 'sql_insert', 'sql_update', 'xlsx'] as const)(
    'refuses non-CSV format %s for a table source instead of silently writing CSV',
    async (dataFormat) => {
      const { delivered, readPage } = capture();
      const exporter = exporterWith(readPage, delivered);

      await expect(exporter.exportTables(REQUEST({ dataFormat }))).rejects.toThrow(
        `'${dataFormat}' format is not supported by the web exporter`,
      );
      expect(readPage).not.toHaveBeenCalled();
      expect(delivered).toHaveLength(0);
    },
  );

  it('exempts structure_only from the format gate (no data file is planned)', async () => {
    const { delivered, readPage } = capture();
    const exporter = exporterWith(readPage, delivered);

    await expect(
      exporter.exportTables(
        REQUEST({
          mode: 'structure_only',
          dataFormat: 'xlsx',
          tables: [{ tableName: 'users', columns: [], ddl: 'CREATE TABLE users (id INT);' }],
        }),
      ),
    ).resolves.toEqual({ Saved: 1 });
    expect(readPage).not.toHaveBeenCalled();
    // The shared default name follows the actual product: a structure-only
    // export is DDL text, so it is named .sql even when the data format
    // is xlsx (same rule as the desktop `default_export_name`).
    expect(delivered[0]?.fileName).toBe('export_structure_2026-11-09-05-00-00.sql');
    expect(delivered[0]?.contents).toContain('CREATE TABLE users (id INT);');
  });

  it('refuses zip output instead of renaming a CSV to .zip', async () => {
    const { delivered, readPage } = capture();
    const exporter = exporterWith(readPage, delivered);

    await expect(exporter.exportTables(REQUEST({ outputMode: 'zip' }))).rejects.toThrow(
      'zip output is not supported by the web exporter',
    );
    expect(readPage).not.toHaveBeenCalled();
    expect(delivered).toHaveLength(0);
  });

  it('refuses a multi-table data export whose desktop plan needs a zip', async () => {
    const { delivered, readPage } = capture();
    const exporter = exporterWith(readPage, delivered);

    await expect(
      exporter.exportTables(
        REQUEST({
          tables: [
            { tableName: 'users', columns: ['id'] },
            { tableName: 'orders', columns: ['id'] },
          ],
        }),
      ),
    ).rejects.toThrow('needs a zip archive, which is not supported by the web exporter');
    expect(readPage).not.toHaveBeenCalled();
    expect(delivered).toHaveLength(0);
  });

  it('refuses data_and_structure in CSV: the desktop plan is two files, not one', async () => {
    const { delivered, readPage } = capture();
    const exporter = exporterWith(readPage, delivered);

    await expect(exporter.exportTables(REQUEST({ mode: 'data_and_structure' }))).rejects.toThrow(
      'needs a zip archive, which is not supported by the web exporter',
    );
    expect(readPage).not.toHaveBeenCalled();
    expect(delivered).toHaveLength(0);
  });

  it('refuses an empty table list', async () => {
    const { delivered, readPage } = capture();
    const exporter = exporterWith(readPage, delivered);

    await expect(exporter.exportTables(REQUEST({ tables: [] }))).rejects.toThrow(
      'no tables selected',
    );
    expect(readPage).not.toHaveBeenCalled();
  });

  it('delivers a single data table as pure CSV (no merge comments)', async () => {
    const { delivered, readPage } = capture();
    const exporter = exporterWith(readPage, delivered);

    await exporter.exportTables(REQUEST());
    expect(delivered[0]?.contents).toBe('id\n1\n');
    expect(delivered[0]?.contents).not.toContain('-- data for');
    expect(delivered[0]?.contents).not.toContain('-- =====');
  });

  it('structure_only across tables concatenates DDL plans with desktop separators', async () => {
    const { delivered, readPage } = capture();
    const exporter = exporterWith(readPage, delivered, async () => 'CREATE TABLE orders (id INT);');

    await exporter.exportTables(
      REQUEST({
        mode: 'structure_only',
        tables: [
          { tableName: 'users', columns: [], ddl: 'CREATE TABLE users (id INT);' },
          { tableName: 'orders', columns: [] },
        ],
      }),
    );

    expect(readPage).not.toHaveBeenCalled();
    expect(delivered[0]?.contents).toBe(
      '-- ===== users.sql =====\n\nCREATE TABLE users (id INT);\n\n' +
        '-- ===== orders.sql =====\n\nCREATE TABLE orders (id INT);\n\n',
    );
  });

  it('uses the shared DDL placeholder when neither table.ddl nor loadDdl has text', async () => {
    const { delivered, readPage } = capture();
    const exporter = exporterWith(readPage, delivered);

    await exporter.exportTables(REQUEST({ mode: 'structure_only' }));

    expect(delivered[0]?.contents).toBe('-- DDL unavailable for users');
  });

  it('prefers the request table.ddl over the injected loadDdl', async () => {
    const { delivered, readPage } = capture();
    const exporter = exporterWith(readPage, delivered, async () => 'CREATE TABLE users (id INT);');

    await exporter.exportTables(
      REQUEST({
        mode: 'structure_only',
        tables: [{ tableName: 'users', columns: [], ddl: 'CREATE TABLE users (id TEXT);' }],
      }),
    );

    expect(delivered[0]?.contents).toBe('CREATE TABLE users (id TEXT);');
  });

  it('refuses structure modes for a sql source without explicit ddl', async () => {
    const { delivered, readPage } = capture();
    const exporter = exporterWith(readPage, delivered);

    await expect(
      exporter.exportTables(
        REQUEST({
          mode: 'structure_only',
          tables: [
            {
              tableName: 'users',
              columns: ['id'],
              source: { type: 'sql', sql: 'SELECT id FROM users' },
            },
          ],
        }),
      ),
    ).rejects.toThrow("structure export for non-table source 'users' needs an explicit ddl");
    expect(readPage).not.toHaveBeenCalled();
    expect(delivered).toHaveLength(0);
  });

  it('accepts a sql source in structure mode when the caller supplies explicit ddl', async () => {
    const { delivered, readPage } = capture();
    const exporter = exporterWith(readPage, delivered);

    await expect(
      exporter.exportTables(
        REQUEST({
          mode: 'structure_only',
          tables: [
            {
              tableName: 'users',
              columns: ['id'],
              ddl: 'CREATE TABLE users (id INT);',
              source: { type: 'sql', sql: 'SELECT id FROM users' },
            },
          ],
        }),
      ),
    ).resolves.toEqual({ Saved: 1 });
    expect(readPage).not.toHaveBeenCalled();
    expect(delivered[0]?.contents).toBe('CREATE TABLE users (id INT);');
  });

  it('refuses structure modes for a provided source without explicit ddl', async () => {
    const { delivered, readPage } = capture();
    const exporter = exporterWith(readPage, delivered);

    await expect(
      exporter.exportTables(
        REQUEST({
          mode: 'structure_only',
          tables: [
            {
              tableName: 'users',
              columns: ['id'],
              source: {
                type: 'provided',
                columns: [{ name: 'id', dataType: 'int', nullable: false }],
                rows: [[1]],
              },
            },
          ],
        }),
      ),
    ).rejects.toThrow("structure export for non-table source 'users' needs an explicit ddl");
    expect(delivered).toHaveLength(0);
  });
});

describe('csvCell alignment with escapeCsvField (item 4)', () => {
  const exportRows = async (rows: unknown[][], columns = ['v']): Promise<string> => {
    const readPage = vi
      .fn<(table: string, cursor: unknown) => Promise<import('../blobTablesExporter').TablePage>>()
      .mockResolvedValue({ columns, rows, nextCursor: null });
    const delivered: string[] = [];
    const exporter = createBlobTablesExporter(readPage, {
      deliver: (contents) => delivered.push(contents as string),
    });
    await exporter.exportTables(REQUEST());
    return delivered[0] ?? '';
  };

  it('keeps the formula-injection guard for every prefix the old rule quoted', async () => {
    const contents = await exportRows([['=SUM(A1)'], ['+2+2'], ['-x'], ['@user']]);
    expect(contents).toContain('"=SUM(A1)"');
    expect(contents).toContain('"+2+2"');
    expect(contents).toContain('"-x"');
    expect(contents).toContain('"@user"');
  });

  it('does not wrap a formula value the standard rule already quoted', async () => {
    const contents = await exportRows([['=a,b']]);
    expect(contents).toContain('"=a,b"');
    expect(contents).not.toContain('""=a,b""');
  });

  it('keeps quoting negative numbers, as the old rule did', async () => {
    const contents = await exportRows([[-5], [0], [42]]);
    expect(contents).toContain('"-5"');
    expect(contents).toContain('\n0\n');
    expect(contents).toContain('\n42\n');
  });

  it('uses the standard escaping for quotes, newlines and objects', async () => {
    const contents = await exportRows([['say "hi"'], ['a\nb'], [{ a: 1 }]]);
    expect(contents).toContain('"say ""hi"""');
    expect(contents).toContain('"a\nb"');
    expect(contents).toContain('"{""a"":1}"');
  });

  it('writes null/undefined as empty and no longer crashes on bigint', async () => {
    const contents = await exportRows([[null], [undefined], [BigInt(10)]]);
    expect(contents).toContain('v\n\n\n10\n');
  });

  it('emits the shared formula-guard fixture byte-identical to the Rust and formatRowsAsText paths (issue 2)', async () => {
    // Shared fixture with the Rust test
    // (`crates/datazen/src/export/fetcher.rs`,
    // `write_data_file_sql_source_applies_csv_formula_guard`) and the
    // `formatRowsAsText` test
    // (`packages/frontend/src/lib/__tests__/exportFormat.test.ts`):
    // the same rows must produce byte-identical CSV in all three
    // paths. Desktop CSV bytes change on purpose here: cells starting
    // with `=`, `+`, `-` or `@` are now quoted (security fix, not a
    // regression).
    const contents = await exportRows([
      ['=SUM(A1:B2)'],
      ['+2+2'],
      ['-x'],
      ['@user'],
      [-5],
      ['=a,b'],
      ['say "hi"'],
      ['line1\nline2'],
      ['plain'],
      [5],
    ]);
    expect(contents).toBe(
      'v\n"=SUM(A1:B2)"\n"+2+2"\n"-x"\n"@user"\n"-5"\n"=a,b"\n"say ""hi"""\n"line1\nline2"\nplain\n5\n',
    );
  });

  it('guards a column name starting with `=` in the header', async () => {
    const contents = await exportRows([['plain']], ['=note']);
    expect(contents).toBe('"=note"\nplain\n');
  });
});

describe('downloadBlob', () => {
  it('triggers an anchor download for the blob URL', () => {
    const clicks: unknown[] = [];
    const created: string[] = [];
    const revoked: string[] = [];
    // Minimal DOM doubles — jsdom has Blob/URL, but click is inert, so spy it.
    const originalCreate = URL.createObjectURL;
    const originalRevoke = URL.revokeObjectURL;
    URL.createObjectURL = ((blob: Blob) => {
      created.push(`${blob.size}`);
      return 'blob:test-url';
    }) as typeof URL.createObjectURL;
    URL.revokeObjectURL = ((url: string) => revoked.push(url)) as typeof URL.revokeObjectURL;
    const anchor = document.createElement('a');
    anchor.click = () => clicks.push(anchor.download);
    vi.spyOn(document, 'createElement').mockReturnValueOnce(anchor);

    downloadBlob('a,b\n1,2\n', 'table.csv');

    expect(clicks).toEqual(['table.csv']);
    expect(anchor.href).toBe('blob:test-url');
    expect(revoked).toContain('blob:test-url');
    URL.createObjectURL = originalCreate;
    URL.revokeObjectURL = originalRevoke;
  });
});

describe('provided source in all formats (task 6, plan §2.8)', () => {
  const providedTable = (rows: (Value | null)[][] = [[1, 'a,b'], [2, null]]) => ({
    tableName: 'users',
    columns: ['id', 'name'],
    source: {
      type: 'provided' as const,
      columns: [
        { name: 'id', dataType: 'int', nullable: false },
        { name: 'name', dataType: 'text', nullable: true },
      ],
      rows,
    },
  });

  const captureParts = () => {
    const delivered: Array<{ contents: BlobPart; fileName: string; mimeType?: string }> = [];
    const readPage = vi
      .fn<(table: string, cursor: unknown) => Promise<import('../blobTablesExporter').TablePage>>();
    return { delivered, readPage };
  };
  const exporterFor = (
    readPage: ReturnType<typeof captureParts>['readPage'],
    delivered: Array<{ contents: BlobPart; fileName: string; mimeType?: string }>,
  ) =>
    createBlobTablesExporter(readPage, {
      deliver: (contents, fileName, mimeType) => delivered.push({ contents, fileName, mimeType }),
      now: PINNED_NOW,
    });

  it.each([
    ['tsv', 'id\tname\n1\ta,b\n2\t', 'export_data_2026-11-09-05-00-00.tsv', 'text/tab-separated-values'],
    [
      'json',
      JSON.stringify(
        [
          { id: 1, name: 'a,b' },
          { id: 2, name: null },
        ],
        null,
        2,
      ),
      'export_data_2026-11-09-05-00-00.json',
      'application/json',
    ],
    [
      'markdown',
      '| id | name |\n| --- | --- |\n| 1 | a,b |\n| 2 |  |',
      'export_data_2026-11-09-05-00-00.md',
      'text/markdown',
    ],
  ] as const)(
    'formats a provided source as %s through the shared formatter, no re-read',
    async (dataFormat, expectedContents, expectedFileName, expectedMimeType) => {
      const { delivered, readPage } = captureParts();
      const exporter = exporterFor(readPage, delivered);

      await expect(
        exporter.exportTables(REQUEST({ dataFormat, tables: [providedTable()] })),
      ).resolves.toEqual({ Saved: 1 });

      expect(readPage).not.toHaveBeenCalled();
      expect(delivered).toHaveLength(1);
      expect(delivered[0]?.contents).toBe(expectedContents);
      expect(delivered[0]?.fileName).toBe(expectedFileName);
      expect(delivered[0]?.mimeType).toBe(expectedMimeType);
    },
  );

  it.each(['sql_insert', 'sql_update'] as const)(
    'formats a provided source as %s with the DDL filename rule',
    async (dataFormat) => {
      const { delivered, readPage } = captureParts();
      const exporter = exporterFor(readPage, delivered);

      await expect(
        exporter.exportTables(REQUEST({ dataFormat, tables: [providedTable()] })),
      ).resolves.toEqual({ Saved: 1 });

      expect(readPage).not.toHaveBeenCalled();
      const contents = delivered[0]?.contents as string;
      expect(contents).toContain('BEGIN;');
      expect(contents).toContain('COMMIT;');
      expect(contents).toContain("'a,b'");
      if (dataFormat === 'sql_insert') {
        expect(contents).toContain('INSERT INTO');
      } else {
        expect(contents).toContain('UPDATE');
        expect(contents).toContain('WHERE');
      }
      expect(delivered[0]?.fileName).toBe('export_data_2026-11-09-05-00-00.sql');
      expect(delivered[0]?.mimeType).toBe('text/sql');
    },
  );

  it('formats a provided source as XLSX and downloads decoded workbook bytes', async () => {
    const { delivered, readPage } = captureParts();
    const exporter = exporterFor(readPage, delivered);

    await expect(
      exporter.exportTables(REQUEST({ dataFormat: 'xlsx', tables: [providedTable()] })),
    ).resolves.toEqual({ Saved: 1 });

    expect(readPage).not.toHaveBeenCalled();
    expect(delivered).toHaveLength(1);
    expect(delivered[0]?.fileName).toBe('export_data_2026-11-09-05-00-00.xlsx');
    expect(delivered[0]?.mimeType).toBe(
      'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
    );
    const bytes = delivered[0]?.contents as Uint8Array;
    expect(bytes).toBeInstanceOf(Uint8Array);
    const workbook = XLSX.read(bytes, { type: 'array' });
    const sheet = workbook.Sheets['Sheet1'];
    expect(sheet).toBeDefined();
    expect(XLSX.utils.sheet_to_json(sheet!)).toEqual([
      { id: 1, name: 'a,b' },
      // A null cell reads back as a missing key — the row itself survives.
      { id: 2 },
    ]);
  });

  it('keeps the formula-injection guard for provided CSV', async () => {
    const { delivered, readPage } = captureParts();
    const exporter = exporterFor(readPage, delivered);

    await exporter.exportTables(
      REQUEST({
        tables: [
          {
            tableName: 'notes',
            columns: ['note'],
            source: {
              type: 'provided',
              columns: [{ name: 'note', dataType: 'text', nullable: true }],
              rows: [['=SUM(A1)'], ['plain']],
            },
          },
        ],
      }),
    );

    expect(readPage).not.toHaveBeenCalled();
    expect(delivered[0]?.contents as string).toContain('"=SUM(A1)"');
    expect(delivered[0]?.contents as string).toContain('plain');
  });

  it('refuses a non-CSV format for a mixed request (provided + table source)', async () => {
    const { delivered, readPage } = captureParts();
    const exporter = exporterFor(readPage, delivered);

    await expect(
      exporter.exportTables(
        REQUEST({
          dataFormat: 'json',
          tables: [providedTable(), { tableName: 'orders', columns: ['id'] }],
        }),
      ),
    ).rejects.toThrow("'json' format is not supported by the web exporter");
    expect(readPage).not.toHaveBeenCalled();
    expect(delivered).toHaveLength(0);
  });

  it('still refuses a sql source in data mode, even when the format is CSV', async () => {
    const { delivered, readPage } = captureParts();
    const exporter = exporterFor(readPage, delivered);

    await expect(
      exporter.exportTables(
        REQUEST({
          tables: [
            {
              tableName: 'users',
              columns: ['id'],
              source: { type: 'sql', sql: 'SELECT id FROM users WHERE id = 1' },
            },
          ],
        }),
      ),
    ).rejects.toThrow('filtered export is not supported by the web exporter');
    expect(readPage).not.toHaveBeenCalled();
    expect(delivered).toHaveLength(0);
  });

  it('prefixes DDL into the same .sql file for provided sql_insert in data_and_structure', async () => {
    const { delivered, readPage } = captureParts();
    const exporter = exporterFor(readPage, delivered);

    await expect(
      exporter.exportTables(
        REQUEST({
          mode: 'data_and_structure',
          dataFormat: 'sql_insert',
          tables: [
            { ...providedTable(), ddl: 'CREATE TABLE users (id INT, name TEXT);' },
          ],
        }),
      ),
    ).resolves.toEqual({ Saved: 1 });

    expect(readPage).not.toHaveBeenCalled();
    const contents = delivered[0]?.contents as string;
    expect(contents).toContain('CREATE TABLE users (id INT, name TEXT);');
    expect(contents).toContain('INSERT INTO');
    expect(delivered[0]?.fileName).toBe('export_full_2026-11-09-05-00-00.sql');
  });

  it('refuses provided data_and_structure in JSON: the desktop plan is two files, not one', async () => {
    const { delivered, readPage } = captureParts();
    const exporter = exporterFor(readPage, delivered);

    await expect(
      exporter.exportTables(
        REQUEST({
          mode: 'data_and_structure',
          dataFormat: 'json',
          tables: [
            { ...providedTable(), ddl: 'CREATE TABLE users (id INT, name TEXT);' },
          ],
        }),
      ),
    ).rejects.toThrow('needs a zip archive, which is not supported by the web exporter');
    expect(readPage).not.toHaveBeenCalled();
    expect(delivered).toHaveLength(0);
  });
});
