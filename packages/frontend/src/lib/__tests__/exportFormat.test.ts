/**
 * Tests for the frontend row formatter (`exportFormat.ts`).
 *
 * Ported from two sources (frontend-DataFetcher plan §2.4 / §5 task 1):
 *
 * - The retired `generateExport` (`e9ace8d` `exportData.ts`) — note there
 *   never was an `exportData.test.ts` in git history (checked: the path
 *   does not exist at `e9ace8d`, nor on any branch), so the ported cases
 *   below are reconstructed from that implementation's code.
 * - The Rust formatter tests (`crates/datazen/src/export/fetcher.rs`,
 *   `format.rs`, `binary.rs`) — same fixtures (`two_cols` / `two_rows`),
 *   same expected text modulo the streaming trailing newline: the Rust
 *   writer ends every row (and the SQL `COMMIT`) with `\n`, while this
 *   in-memory formatter joins lines with `\n` and adds none at the end
 *   (the old `generateExport` shape). Parity assertions therefore compare
 *   `content + '\n'` against the Rust file bytes where noted.
 */
import { describe, expect, it } from 'vitest';
import * as XLSX from 'xlsx';
import {
  buildProvidedExportFilename,
  formatRowsAsText,
  formatRowsAsXlsx,
  SQL_INSERT_BATCH_SIZE,
  type ExportColumn,
  type FormatRowsTextOptions,
} from '../exportFormat';

const usersColumns: ExportColumn[] = [
  { name: 'id', dataType: 'int4', nullable: false, isPrimaryKey: true },
  { name: 'name', dataType: 'text', nullable: true },
];

/** Rust `two_rows`: `[1, 'a'], [2, NULL]`. */
const twoRows: unknown[][] = [
  [1, 'a'],
  [2, null],
];

function textOptions(
  extra: Partial<FormatRowsTextOptions> = {},
): FormatRowsTextOptions {
  return {
    tableName: 'users',
    columns: usersColumns,
    rows: twoRows,
    format: 'csv',
    ...extra,
  };
}

describe('formatRowsAsText: CSV', () => {
  it('writes the Rust two-row fixture (parity with write_data_file_sql_source_writes_csv)', () => {
    const result = formatRowsAsText(textOptions({ format: 'csv' }));
    expect(result.extension).toBe('csv');
    expect(result.mimeType).toBe('text/csv');
    expect(result.content).toBe('id,name\n1,a\n2,');
    expect(`${result.content}\n`).toBe('id,name\n1,a\n2,\n');
  });

  it('quotes fields holding a comma, a quote, or a line break', () => {
    const result = formatRowsAsText(
      textOptions({
        columns: [{ name: 'a,b', dataType: 'text', nullable: true }],
        rows: [['x,y'], ['say "hi"'], ['line1\nline2'], [null], [42], [true]],
        format: 'csv',
      }),
    );
    expect(result.content).toBe(
      '"a,b"\n"x,y"\n"say ""hi"""\n"line1\nline2"\n\n42\ntrue',
    );
  });

  it('renders an empty row set as header only', () => {
    expect(formatRowsAsText(textOptions({ rows: [], format: 'csv' })).content).toBe(
      'id,name',
    );
  });

  it('reads a short row as null and ignores extra values', () => {
    const result = formatRowsAsText(
      textOptions({ rows: [[1], [2, 'b', 'EXTRA']], format: 'csv' }),
    );
    expect(result.content).toBe('id,name\n1,\n2,b');
  });
});

describe('formatRowsAsText: TSV', () => {
  it('flattens tabs and newlines to spaces (parity with write_data_file_tsv_flattens_tabs_and_newlines)', () => {
    const result = formatRowsAsText(
      textOptions({
        rows: [
          [1, 'a\tb\nc'],
          [2, null],
        ],
        format: 'tsv',
      }),
    );
    expect(result.extension).toBe('tsv');
    expect(result.mimeType).toBe('text/tab-separated-values');
    expect(result.content).toBe('id\tname\n1\ta b c\n2\t');
    expect(`${result.content}\n`).toBe('id\tname\n1\ta b c\n2\t\n');
  });

  it('drops carriage returns', () => {
    const result = formatRowsAsText(
      textOptions({
        columns: [{ name: 'v', dataType: 'text', nullable: true }],
        rows: [['a\r\nb']],
        format: 'tsv',
      }),
    );
    expect(result.content).toBe('v\na b');
  });

  it('renders an object cell as its JSON text', () => {
    const result = formatRowsAsText(
      textOptions({
        columns: [{ name: 'v', dataType: 'jsonb', nullable: true }],
        rows: [[{ k: 1 }]],
        format: 'tsv',
      }),
    );
    expect(result.content).toBe('v\n{"k":1}');
  });
});

describe('formatRowsAsText: JSON', () => {
  it('pretty-prints row objects with null for missing values (parity with write_data_file_provided_source_never_queries_backend)', () => {
    const result = formatRowsAsText(textOptions({ format: 'json' }));
    expect(result.extension).toBe('json');
    expect(result.mimeType).toBe('application/json');
    expect(result.content).toBe(
      JSON.stringify(
        [
          { id: 1, name: 'a' },
          { id: 2, name: null },
        ],
        null,
        2,
      ),
    );
    expect(JSON.parse(result.content)).toEqual([
      { id: 1, name: 'a' },
      { id: 2, name: null },
    ]);
  });

  it('renders an empty row set as an empty array', () => {
    expect(formatRowsAsText(textOptions({ rows: [], format: 'json' })).content).toBe(
      '[]',
    );
  });

  it('keeps safe bigints numeric, degrades huge ones to strings, and nulls undefined', () => {    const result = formatRowsAsText(
      textOptions({
        columns: [
          { name: 'safe', dataType: 'int8', nullable: true },
          { name: 'huge', dataType: 'int8', nullable: true },
          { name: 'missing', dataType: 'text', nullable: true },
          { name: 'nested', dataType: 'jsonb', nullable: true },
        ],
        rows: [[42n, 9_007_199_254_740_993n, undefined, { n: 9_007_199_254_740_993n }]],
        format: 'json',
      }),
    );
    expect(JSON.parse(result.content)).toEqual([
      { safe: 42, huge: '9007199254740993', missing: null, nested: { n: '9007199254740993' } },
    ]);
  });

  it('serialises a Date cell to its ISO string', () => {
    const result = formatRowsAsText(
      textOptions({
        columns: [{ name: 'created', dataType: 'timestamp', nullable: true }],
        rows: [[new Date('2026-11-09T12:00:00.000Z')]],
        format: 'json',
      }),
    );
    expect(JSON.parse(result.content)).toEqual([{ created: '2026-11-09T12:00:00.000Z' }]);
  });

  it('serialises a Uint8Array cell as a number array', () => {
    const result = formatRowsAsText(
      textOptions({
        columns: [{ name: 'payload', dataType: 'bytea', nullable: true }],
        rows: [[new Uint8Array([0x00, 0xff])]],
        format: 'json',
      }),
    );
    expect(JSON.parse(result.content)).toEqual([{ payload: [0, 255] }]);
  });

  it('keeps an array cell as an array', () => {
    const result = formatRowsAsText(
      textOptions({
        columns: [{ name: 'tags', dataType: 'text[]', nullable: true }],
        rows: [[['a', 'b']]],
        format: 'json',
      }),
    );
    expect(JSON.parse(result.content)).toEqual([{ tags: ['a', 'b'] }]);
  });
});

describe('formatRowsAsText: Markdown', () => {
  it('escapes pipes and breaks like the clipboard serializer (parity with write_data_file_markdown_escapes_pipes_and_breaks)', () => {
    const result = formatRowsAsText(
      textOptions({ rows: [[1, 'a|b\nc\\d']], format: 'markdown' }),
    );
    expect(result.extension).toBe('md');
    expect(result.mimeType).toBe('text/markdown');
    expect(result.content).toBe('| id | name |\n| --- | --- |\n| 1 | a\\|b<br>c\\\\d |');
    expect(`${result.content}\n`).toBe(
      '| id | name |\n| --- | --- |\n| 1 | a\\|b<br>c\\\\d |\n',
    );
  });

  it('renders an empty row set as header plus separator', () => {
    expect(formatRowsAsText(textOptions({ rows: [], format: 'markdown' })).content).toBe(
      '| id | name |\n| --- | --- |',
    );
  });
});

describe('formatRowsAsText: SQL INSERT', () => {
  it('wraps batched INSERTs in a transaction (postgres quoting)', () => {
    const result = formatRowsAsText(
      textOptions({ format: 'sql_insert', databaseType: 'postgresql' }),
    );
    expect(result.extension).toBe('sql');
    expect(result.mimeType).toBe('text/sql');
    expect(result.content).toBe(
      'BEGIN;\n' +
        'INSERT INTO "users" ("id", "name") VALUES\n' +
        "  (1, 'a'),\n" +
        '  (2, NULL);\n' +
        'COMMIT;',
    );
  });

  it('doubles single quotes, renders booleans and objects as literals', () => {
    const result = formatRowsAsText(
      textOptions({
        columns: [
          { name: 'id', dataType: 'int4', nullable: false },
          { name: 'name', dataType: 'text', nullable: true },
          { name: 'active', dataType: 'bool', nullable: true },
          { name: 'meta', dataType: 'jsonb', nullable: true },
        ],
        rows: [[1, "O'Brien", true, { k: 1 }]],
        format: 'sql_insert',
        databaseType: 'postgresql',
      }),
    );
    expect(result.content).toContain("(1, 'O''Brien', TRUE, '{\"k\":1}')");
  });

  it('doubles quotes inside identifiers', () => {
    const result = formatRowsAsText(
      textOptions({
        tableName: 'my"table',
        columns: [{ name: 'a"b', dataType: 'text', nullable: true }],
        rows: [['x']],
        format: 'sql_insert',
        databaseType: 'postgresql',
      }),
    );
    expect(result.content).toContain('INSERT INTO "my""table" ("a""b") VALUES');
  });

  it('splits after SQL_INSERT_BATCH_SIZE rows', () => {
    expect(SQL_INSERT_BATCH_SIZE).toBe(500);
    const rows = Array.from({ length: 501 }, (_, i) => [i + 1, 'x']);
    const result = formatRowsAsText(
      textOptions({ rows, format: 'sql_insert', databaseType: 'postgresql' }),
    );
    expect(result.content.match(/INSERT INTO/g)).toHaveLength(2);
    expect(result.content).toContain('  (500, \'x\');');
    expect(result.content).toContain('  (501, \'x\');');
  });

  it('renders an empty row set as an empty transaction', () => {
    expect(
      formatRowsAsText(textOptions({ rows: [], format: 'sql_insert' })).content,
    ).toBe('BEGIN;\nCOMMIT;');
  });

  it('uses backticks and backslash escaping for mysql', () => {
    const result = formatRowsAsText(
      textOptions({
        rows: [[1, 'a\\b']],
        format: 'sql_insert',
        databaseType: 'mysql',
      }),
    );
    expect(result.content).toBe(
      'BEGIN;\n' +
        'INSERT INTO `users` (`id`, `name`) VALUES\n' +
        "  (1, 'a\\\\b');\n" +
        'COMMIT;',
    );
  });

  it('uses double quotes and leaves backslashes alone for sqlite', () => {
    const result = formatRowsAsText(
      textOptions({
        rows: [[1, 'a\\b']],
        format: 'sql_insert',
        databaseType: 'sqlite',
      }),
    );
    expect(result.content).toContain("(1, 'a\\b')");
    expect(result.content).toContain('INSERT INTO "users" ("id", "name")');
  });

  it('uses bracket quoting and BEGIN TRANSACTION for sqlserver', () => {
    const result = formatRowsAsText(
      textOptions({
        columns: [
          { name: 'id', dataType: 'int', nullable: false },
          { name: 'we]ird', dataType: 'text', nullable: true },
        ],
        rows: [[1, 'x']],
        format: 'sql_insert',
        databaseType: 'sqlserver',
      }),
    );
    expect(result.content).toBe(
      'BEGIN TRANSACTION;\n' +
        'INSERT INTO [users] ([id], [we]]ird]) VALUES\n' +
        "  (1, 'x');\n" +
        'COMMIT;',
    );
  });
});

describe('SQL dialect table pins to the driver registry', () => {
  it('quotes identifiers and escapes literals per DB_REGISTRY for every driver', async () => {
    // The formatter keeps a static per-dialect table instead of importing
    // `databaseTypes` (it must stay a leaf — see `exportFormat.ts`). This
    // test pins that table against the registry itself, so a driver meta
    // that moves a quote char or escaper fails here instead of drifting.
    // `databaseTypes` is imported only by this test file, never by the
    // formatter, so setup still cannot reach the generated registry.
    const { DB_REGISTRY, defaultEscapeSqlValue } = await import('../databaseTypes');
    // Distinguishes the default escaper from the MySQL-family one: the
    // default leaves backslashes and double quotes alone, MySQL doubles
    // backslashes and backslash-escapes double quotes; both double
    // single quotes.
    const probe = 'a\\b"c\'d';
    for (const [dbType, meta] of Object.entries(DB_REGISTRY)) {
      const result = formatRowsAsText(
        textOptions({
          rows: [[1, probe]],
          format: 'sql_insert',
          databaseType: dbType,
        }),
      );
      const q = meta.quoteChar ?? '"';
      const quoted = (name: string): string =>
        q === '' ? name : q === '[' ? `[${name}]` : `${q}${name}${q}`;
      expect(result.content).toContain(
        `INSERT INTO ${quoted('users')} (${quoted('id')}, ${quoted('name')})`,
      );
      const expectedLiteral = meta.escapeSqlValue
        ? meta.escapeSqlValue(probe)
        : defaultEscapeSqlValue(probe);
      expect(result.content).toContain(`(1, ${expectedLiteral})`);
    }
  });
});

describe('formatRowsAsText: SQL UPDATE', () => {
  it('uses the isPrimaryKey column (parity with write_data_file_sql_update_uses_primary_key)', () => {
    const result = formatRowsAsText(
      textOptions({ format: 'sql_update', databaseType: 'postgresql' }),
    );
    expect(result.content).toBe(
      'BEGIN;\n' +
        'UPDATE "users" SET "name" = \'a\' WHERE "id" = 1;\n' +
        'UPDATE "users" SET "name" = NULL WHERE "id" = 2;\n' +
        'COMMIT;',
    );
  });

  it('falls back to the first column without a primary key (parity with write_data_file_sql_update_falls_back_to_first_column)', () => {
    const result = formatRowsAsText(
      textOptions({
        columns: [
          { name: 'id', dataType: 'int4', nullable: false },
          { name: 'name', dataType: 'text', nullable: true },
        ],
        format: 'sql_update',
        databaseType: 'postgresql',
      }),
    );
    expect(result.content).toContain('WHERE "id" = 1;');
  });

  it('lets an explicit primaryKey override the isPrimaryKey column', () => {
    const result = formatRowsAsText(
      textOptions({
        format: 'sql_update',
        databaseType: 'postgresql',
        primaryKey: 'name',
      }),
    );
    expect(result.content).toContain('SET "id" = 1 WHERE "name" = \'a\';');
  });

  it('ignores a primaryKey that is not among the columns', () => {
    const result = formatRowsAsText(
      textOptions({
        format: 'sql_update',
        databaseType: 'postgresql',
        primaryKey: 'nope',
      }),
    );
    expect(result.content).toContain('WHERE "id" = 1;');
  });

  it('uses mysql backticks and sqlserver brackets plus BEGIN TRANSACTION', () => {
    const mysql = formatRowsAsText(
      textOptions({ format: 'sql_update', databaseType: 'mysql' }),
    );
    expect(mysql.content).toContain('UPDATE `users` SET `name` = \'a\' WHERE `id` = 1;');
    const sqlserver = formatRowsAsText(
      textOptions({ format: 'sql_update', databaseType: 'sqlserver' }),
    );
    expect(sqlserver.content.startsWith('BEGIN TRANSACTION;\n')).toBe(true);
    expect(sqlserver.content).toContain(
      'UPDATE [users] SET [name] = \'a\' WHERE [id] = 1;',
    );
  });

  it('renders an empty row set as an empty transaction', () => {
    expect(
      formatRowsAsText(textOptions({ rows: [], format: 'sql_update' })).content,
    ).toBe('BEGIN;\nCOMMIT;');
  });

  it('rejects a single-column table (parity with write_data_file_sql_update_single_column_is_rejected)', () => {
    expect(() =>
      formatRowsAsText(
        textOptions({
          columns: [{ name: 'id', dataType: 'int4', nullable: false }],
          rows: [[1]],
          format: 'sql_update',
        }),
      ),
    ).toThrow('sql_update export needs at least one non-key column to SET');
  });
});

describe('formatRowsAsText: guards', () => {
  it('rejects an empty column list', () => {
    expect(() =>
      formatRowsAsText(textOptions({ columns: [], format: 'csv' })),
    ).toThrow('export needs at least one column');
    expect(() =>
      formatRowsAsXlsx({ tableName: 'users', columns: [], rows: [] }),
    ).toThrow('export needs at least one column');
  });

  it('refuses xlsx through the text entry point', () => {
    expect(() =>
      formatRowsAsText(textOptions({ format: 'xlsx' as never })),
    ).toThrow("formatRowsAsText cannot format 'xlsx' as text");
  });
});

describe('formatRowsAsXlsx', () => {
  function readBack(dataBase64: string) {
    const wb = XLSX.read(dataBase64, { type: 'base64', cellDates: true });
    const sheet = wb.Sheets['Sheet1'];
    return XLSX.utils.sheet_to_json<unknown[]>(sheet, { header: 1, defval: null });
  }

  it('round-trips headers and value kinds (parity with xlsx_roundtrip_header_rows_and_value_kinds)', () => {
    const result = formatRowsAsXlsx({
      tableName: 'users',
      columns: [
        { name: 'id', dataType: 'int8', nullable: false },
        { name: 'score', dataType: 'float8', nullable: true },
        { name: 'name', dataType: 'text', nullable: true },
        { name: 'active', dataType: 'bool', nullable: true },
        { name: 'payload', dataType: 'bytea', nullable: true },
        { name: 'created', dataType: 'timestamp', nullable: true },
        { name: 'meta', dataType: 'jsonb', nullable: true },
        { name: 'note', dataType: 'text', nullable: true },
      ],
      rows: [
        [
          42,
          1.5,
          'a,b',
          true,
          new Uint8Array([0x00, 0xff]),
          new Date('2026-11-09T12:00:00.000Z'),
          { k: 1 },
          null,
        ],
      ],
    });
    expect(result.extension).toBe('xlsx');
    expect(result.mimeType).toBe(
      'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
    );
    // An XLSX workbook is a ZIP: base64 of the PK magic.
    expect(result.dataBase64.startsWith('UEsDB')).toBe(true);
    const matrix = readBack(result.dataBase64);
    expect(matrix[0]).toEqual([
      'id',
      'score',
      'name',
      'active',
      'payload',
      'created',
      'meta',
      'note',
    ]);
    const row = matrix[1]!;
    expect(row[0]).toBe(42);
    expect(row[1]).toBe(1.5);
    expect(row[2]).toBe('a,b');
    expect(row[3]).toBe(true);
    expect(row[4]).toBe('00ff');
    expect(row[5]).toBeInstanceOf(Date);
    expect(row[6]).toBe('{"k":1}');
    expect(row[7]).toBe(null);
  });

  it('keeps safe bigints numeric and degrades huge ones to strings', () => {
    const result = formatRowsAsXlsx({
      tableName: 't',
      columns: [
        { name: 'safe', dataType: 'int8', nullable: true },
        { name: 'huge', dataType: 'int8', nullable: true },
      ],
      rows: [[42n, 9_007_199_254_740_993n]],
    });
    const matrix = readBack(result.dataBase64);
    expect(matrix[1]).toEqual([42, '9007199254740993']);
  });

  it('renders an empty row set as header only', () => {
    const result = formatRowsAsXlsx({
      tableName: 'users',
      columns: usersColumns,
      rows: [],
    });
    expect(readBack(result.dataBase64)).toEqual([['id', 'name']]);
  });

  it('refuses a non-xlsx format argument', () => {
    expect(() =>
      formatRowsAsXlsx({
        tableName: 'users',
        columns: usersColumns,
        rows: twoRows,
        format: 'csv' as unknown as 'xlsx',
      }),
    ).toThrow("formatRowsAsXlsx only handles 'xlsx', got 'csv'");
  });
});

describe('buildProvidedExportFilename', () => {
  it('names one file per table with the format extension (build_file_plans data-only rule)', () => {
    expect(buildProvidedExportFilename('users', 'csv')).toBe('users.csv');
    expect(buildProvidedExportFilename('users', 'tsv')).toBe('users.tsv');
    expect(buildProvidedExportFilename('users', 'json')).toBe('users.json');
    expect(buildProvidedExportFilename('users', 'markdown')).toBe('users.md');
    expect(buildProvidedExportFilename('users', 'sql_insert')).toBe('users.sql');
    expect(buildProvidedExportFilename('users', 'sql_update')).toBe('users.sql');
    expect(buildProvidedExportFilename('users', 'xlsx')).toBe('users.xlsx');
  });
});
