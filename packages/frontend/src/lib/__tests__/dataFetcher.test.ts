/**
 * Tests for the frontend `DataFetcher` (`dataFetcher.ts`, plan §2.1/§2.2,
 * task 2).
 *
 * Mocking follows the established patterns in this package:
 * - `IS_DESKTOP_BUILD` flips through the getter mock in
 *   `__tests__/updater.test.ts` (compile-time `platform/env` constant).
 * - The save-dialog commands are mocked the way
 *   `lib/chart/__tests__/export.test.ts` mocks `commands/file` — so the
 *   test can assert exactly which formatted bytes reach the save call,
 *   and that no other backend call is made: the mock exposes only the
 *   two dumb save functions, and `downloadBlob` is spied the same way.
 *
 * The hard constraint under test: `ProvidedDataFetcher` never invokes a
 * row-carrying Rust command. The save mocks receive a formatted *string*
 * (or base64 for XLSX) plus filename/filter/extensions — never the rows
 * — and nothing else is called.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import * as XLSX from 'xlsx';

const envState = vi.hoisted(() => ({ desktop: true }));
vi.mock('../../platform/env', () => ({
  get IS_DESKTOP_BUILD() {
    return envState.desktop;
  },
}));

const saveTextMock = vi.hoisted(() => vi.fn());
const saveBase64Mock = vi.hoisted(() => vi.fn());
const exportTablesMock = vi.hoisted(() => vi.fn());
const unlistenMock = vi.hoisted(() => vi.fn());
const progressHandlerSlot = vi.hoisted(() => ({
  current: null as ((event: { table: string; rowsWritten: number }) => void) | null,
}));
const subscribeProgressMock = vi.hoisted(() => vi.fn());
vi.mock('../../commands/file', () => ({
  fileCommands: {
    saveTextWithDialog: (...args: unknown[]) => saveTextMock(...args),
    saveBase64WithDialog: (...args: unknown[]) => saveBase64Mock(...args),
  },
  onExportProgress: (...args: unknown[]) => subscribeProgressMock(...args),
}));
vi.mock('../../platform/contracts', () => ({
  requireTablesExporter: () => ({
    exportTables: (...args: unknown[]) => exportTablesMock(...args),
  }),
}));

const downloadBlobMock = vi.hoisted(() => vi.fn());
vi.mock('../../platform/blobTablesExporter', async (importOriginal) => {
  const actual =
    await importOriginal<typeof import('../../platform/blobTablesExporter')>();
  return { ...actual, downloadBlob: (...args: unknown[]) => downloadBlobMock(...args) };
});

import {
  ProvidedDataFetcher,
  SqlDataFetcher,
  resolveDataFetcher,
  type ExportContext,
  type ExportProgress,
} from '../dataFetcher';
import type { ExportTablesRequest } from '../../commands/file';
import type { ExportColumn } from '../exportFormat';

const usersColumns: ExportColumn[] = [
  { name: 'id', dataType: 'int4', nullable: false, isPrimaryKey: true },
  { name: 'name', dataType: 'text', nullable: true },
];

const twoRows: unknown[][] = [
  [1, 'a'],
  [2, null],
];

function providedContext(
  extra: Partial<ExportContext> = {},
): ExportContext {
  return {
    tableName: 'users',
    scope: 'current_page',
    columns: usersColumns,
    rows: twoRows,
    dataFormat: 'csv',
    ...extra,
  };
}

describe('resolveDataFetcher', () => {
  it('resolves current_page with rows to ProvidedDataFetcher, without a session', () => {
    const fetcher = resolveDataFetcher(providedContext());
    expect(fetcher).toBeInstanceOf(ProvidedDataFetcher);
    expect(fetcher.needsBackend).toBe(false);
  });

  it('resolves current_page with rows to ProvidedDataFetcher even with a session', () => {
    const fetcher = resolveDataFetcher(
      providedContext({ dbSessionId: 'session-1' }),
    );
    expect(fetcher).toBeInstanceOf(ProvidedDataFetcher);
    expect(fetcher.needsBackend).toBe(false);
  });

  it('resolves selected with rows to ProvidedDataFetcher, with or without a session', () => {
    expect(resolveDataFetcher(providedContext({ scope: 'selected' }))).toBeInstanceOf(
      ProvidedDataFetcher,
    );
    expect(
      resolveDataFetcher(providedContext({ scope: 'selected', dbSessionId: 's1' })),
    ).toBeInstanceOf(ProvidedDataFetcher);
  });

  it('throws when a provided scope has no rows', () => {
    expect(() =>
      resolveDataFetcher(providedContext({ rows: undefined })),
    ).toThrow("provided export for table 'users' has no loaded rows");
  });

  it('throws an unresolvable error for entire_table without a session', () => {
    expect(() =>
      resolveDataFetcher(providedContext({ scope: 'entire_table', rows: undefined })),
    ).toThrow(
      "cannot resolve an exporter for table 'users' (scope 'entire_table'): " +
        'entire-table and filtered exports need a database session',
    );
  });

  it('throws an unresolvable error for filtered_table without a session', () => {
    expect(() =>
      resolveDataFetcher(
        providedContext({ scope: 'filtered_table', rows: undefined, dbSessionId: '' }),
      ),
    ).toThrow('need a database session');
  });

  it('resolves entire_table with a session to SqlDataFetcher', () => {
    const fetcher = resolveDataFetcher(
      providedContext({ scope: 'entire_table', rows: undefined, dbSessionId: 's1' }),
    );
    expect(fetcher).toBeInstanceOf(SqlDataFetcher);
    expect(fetcher.needsBackend).toBe(true);
  });

  it('resolves filtered_table with a session to SqlDataFetcher', () => {
    const fetcher = resolveDataFetcher(
      providedContext({
        scope: 'filtered_table',
        rows: undefined,
        dbSessionId: 's1',
        sql: 'SELECT * FROM users WHERE id > 1',
      }),
    );
    expect(fetcher).toBeInstanceOf(SqlDataFetcher);
    expect(fetcher.needsBackend).toBe(true);
  });
});

describe('ProvidedDataFetcher.fetch on desktop', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    envState.desktop = true;
    saveTextMock.mockResolvedValue(true);
    saveBase64Mock.mockResolvedValue(true);
  });

  it('exports current-page CSV with no session: formatted text + filename to the save dialog', async () => {
    const fetcher = resolveDataFetcher(providedContext());
    await expect(fetcher.fetch()).resolves.toBe('saved');

    // The only backend touch is the dumb save of already-formatted text:
    // no rows object, no export_tables_stream, no session anywhere.
    expect(saveTextMock).toHaveBeenCalledTimes(1);
    expect(saveTextMock).toHaveBeenCalledWith('id,name\n1,a\n2,', 'users.csv', 'CSV', [
      'csv',
    ]);
    expect(typeof saveTextMock.mock.calls[0]![0]).toBe('string');
    expect(saveBase64Mock).not.toHaveBeenCalled();
    expect(downloadBlobMock).not.toHaveBeenCalled();
  });

  it('exports XLSX as base64 through the binary save dialog', async () => {
    const fetcher = resolveDataFetcher(providedContext({ dataFormat: 'xlsx' }));
    await expect(fetcher.fetch()).resolves.toBe('saved');

    expect(saveBase64Mock).toHaveBeenCalledTimes(1);
    const [base64, filename, filter, extensions] = saveBase64Mock.mock.calls[0]!;
    expect(filename).toBe('users.xlsx');
    expect(filter).toBe('XLSX');
    expect(extensions).toEqual(['xlsx']);
    expect(typeof base64).toBe('string');
    expect((base64 as string).startsWith('UEsDB')).toBe(true);
    const wb = XLSX.read(base64 as string, { type: 'base64' });
    const matrix = XLSX.utils.sheet_to_json<unknown[]>(wb.Sheets['Sheet1']!, {
      header: 1,
      defval: null,
    });
    expect(matrix).toEqual([
      ['id', 'name'],
      [1, 'a'],
      [2, null],
    ]);
    expect(saveTextMock).not.toHaveBeenCalled();
    expect(downloadBlobMock).not.toHaveBeenCalled();
  });

  it('formats SQL INSERT with the databaseType dialect', async () => {
    const fetcher = resolveDataFetcher(
      providedContext({ dataFormat: 'sql_insert', databaseType: 'postgresql' }),
    );
    await expect(fetcher.fetch()).resolves.toBe('saved');
    expect(saveTextMock).toHaveBeenCalledWith(
      'BEGIN;\nINSERT INTO "users" ("id", "name") VALUES\n  (1, \'a\'),\n  (2, NULL);\nCOMMIT;',
      'users.sql',
      'SQL',
      ['sql'],
    );
  });

  it('returns cancelled and emits no end event when the dialog is dismissed', async () => {
    saveTextMock.mockResolvedValueOnce(false);
    const fetcher = resolveDataFetcher(providedContext());
    const seen: ExportProgress[] = [];
    fetcher.onProgress((p) => seen.push(p));
    await expect(fetcher.fetch()).resolves.toBe('cancelled');
    expect(seen).toEqual([{ table: 'users', rowsWritten: 0, rowsTotal: 2 }]);
  });

  it('emits start/end progress with rowsTotal, to every subscriber', async () => {
    const fetcher = resolveDataFetcher(providedContext());
    const first: ExportProgress[] = [];
    const second: ExportProgress[] = [];
    fetcher.onProgress((p) => first.push(p));
    fetcher.onProgress((p) => second.push(p));
    await expect(fetcher.fetch()).resolves.toBe('saved');
    const expected: ExportProgress[] = [
      { table: 'users', rowsWritten: 0, rowsTotal: 2 },
      { table: 'users', rowsWritten: 2, rowsTotal: 2 },
    ];
    expect(first).toEqual(expected);
    expect(second).toEqual(expected);
  });

  it('stops emitting to a subscriber after unsubscribe', async () => {
    const fetcher = resolveDataFetcher(providedContext());
    const kept: ExportProgress[] = [];
    const dropped: ExportProgress[] = [];
    const unsubscribe = fetcher.onProgress((p) => dropped.push(p));
    fetcher.onProgress((p) => kept.push(p));
    unsubscribe();
    await expect(fetcher.fetch()).resolves.toBe('saved');
    expect(dropped).toEqual([]);
    expect(kept).toHaveLength(2);
  });

  it('exports an empty row set as header only, with rowsTotal 0', async () => {
    const fetcher = resolveDataFetcher(providedContext({ rows: [] }));
    const seen: ExportProgress[] = [];
    fetcher.onProgress((p) => seen.push(p));
    await expect(fetcher.fetch()).resolves.toBe('saved');
    expect(saveTextMock).toHaveBeenCalledWith('id,name', 'users.csv', 'CSV', ['csv']);
    expect(seen).toEqual([
      { table: 'users', rowsWritten: 0, rowsTotal: 0 },
      { table: 'users', rowsWritten: 0, rowsTotal: 0 },
    ]);
  });

  it('rejects when validation fails and saves nothing (duplicate columns)', async () => {
    const fetcher = resolveDataFetcher(
      providedContext({
        columns: [
          { name: 'id', dataType: 'int4', nullable: false },
          { name: 'id', dataType: 'text', nullable: true },
        ],
      }),
    );
    await expect(fetcher.fetch()).rejects.toThrow(
      "provided data source 'users' has a duplicate column name 'id'",
    );
    expect(saveTextMock).not.toHaveBeenCalled();
    expect(saveBase64Mock).not.toHaveBeenCalled();
    expect(downloadBlobMock).not.toHaveBeenCalled();
  });

  it('rejects when validation fails and saves nothing (ragged row)', async () => {
    const fetcher = resolveDataFetcher(providedContext({ rows: [[1]] }));
    await expect(fetcher.fetch()).rejects.toThrow(
      "provided data source 'users' row 0 has 1 values but the schema has 2 columns",
    );
    expect(saveTextMock).not.toHaveBeenCalled();
  });

  it('rejects when validation fails and saves nothing (no columns)', async () => {
    const fetcher = resolveDataFetcher(providedContext({ columns: [] }));
    await expect(fetcher.fetch()).rejects.toThrow(
      "provided data source 'users' has no columns (empty schema)",
    );
    expect(saveTextMock).not.toHaveBeenCalled();
  });
});

describe('ProvidedDataFetcher.fetch on web', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    envState.desktop = false;
  });

  it('downloads CSV as a Blob instead of calling the save dialog', async () => {
    const fetcher = resolveDataFetcher(providedContext());
    await expect(fetcher.fetch()).resolves.toBe('saved');
    expect(downloadBlobMock).toHaveBeenCalledTimes(1);
    expect(downloadBlobMock).toHaveBeenCalledWith(
      'id,name\n1,a\n2,',
      'users.csv',
      'text/csv',
    );
    expect(saveTextMock).not.toHaveBeenCalled();
    expect(saveBase64Mock).not.toHaveBeenCalled();
  });

  it('downloads XLSX as decoded bytes through the Blob path', async () => {
    const fetcher = resolveDataFetcher(providedContext({ dataFormat: 'xlsx' }));
    await expect(fetcher.fetch()).resolves.toBe('saved');
    expect(downloadBlobMock).toHaveBeenCalledTimes(1);
    const [bytes, filename, mimeType] = downloadBlobMock.mock.calls[0]!;
    expect(filename).toBe('users.xlsx');
    expect(mimeType).toBe(
      'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
    );
    expect(bytes).toBeInstanceOf(Uint8Array);
    const wb = XLSX.read(bytes as Uint8Array, { type: 'array' });
    const matrix = XLSX.utils.sheet_to_json<unknown[]>(wb.Sheets['Sheet1']!, {
      header: 1,
      defval: null,
    });
    expect(matrix).toEqual([
      ['id', 'name'],
      [1, 'a'],
      [2, null],
    ]);
    expect(saveTextMock).not.toHaveBeenCalled();
    expect(saveBase64Mock).not.toHaveBeenCalled();
  });

  it('still emits start/end progress on the web path', async () => {
    const fetcher = resolveDataFetcher(providedContext());
    const seen: ExportProgress[] = [];
    fetcher.onProgress((p) => seen.push(p));
    await expect(fetcher.fetch()).resolves.toBe('saved');
    expect(seen).toEqual([
      { table: 'users', rowsWritten: 0, rowsTotal: 2 },
      { table: 'users', rowsWritten: 2, rowsTotal: 2 },
    ]);
  });
});

describe('SqlDataFetcher.fetch (plan §2.3, task 3)', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    progressHandlerSlot.current = null;
    exportTablesMock.mockResolvedValue({ Saved: 3 });
    subscribeProgressMock.mockImplementation(async (handler) => {
      progressHandlerSlot.current =
        handler as (event: { table: string; rowsWritten: number }) => void;
      return unlistenMock;
    });
  });

  function sqlContext(extra: Partial<ExportContext> = {}): ExportContext {
    return {
      tableName: 'users',
      scope: 'entire_table',
      dbSessionId: 's1',
      databaseType: 'postgres',
      columnNames: ['id', 'name'],
      dataFormat: 'csv',
      ...extra,
    };
  }

  function request(): ExportTablesRequest {
    return exportTablesMock.mock.calls[0]![0] as ExportTablesRequest;
  }

  it('sends a single-table request identical to the old exportSingleTableViaRust', async () => {
    const fetcher = new SqlDataFetcher(sqlContext());
    await expect(fetcher.fetch()).resolves.toBe('saved');

    expect(exportTablesMock).toHaveBeenCalledTimes(1);
    expect(request()).toEqual({
      dbSessionId: 's1',
      // One per-run token (minted at construction; the value is opaque).
      cancelToken: expect.any(String),
      databaseType: 'postgres',
      database: undefined,
      schema: undefined,
      mode: 'data_only',
      dataFormat: 'csv',
      outputMode: 'single',
      tables: [
        {
          tableName: 'users',
          schema: undefined,
          columns: ['id', 'name'],
          source: { type: 'table' },
          primaryKey: undefined,
        },
      ],
    });
  });

  it('derives column names from ExportColumn entries when columnNames is absent', async () => {
    const { columnNames: _dropped, ...ctx } = sqlContext({ columns: usersColumns });
    await new SqlDataFetcher(ctx).fetch();
    expect(request().tables[0]!.columns).toEqual(['id', 'name']);
  });

  it('sends a SQL source carrying the caller-built statement for filtered tables', async () => {
    const sql = "SELECT \"id\" FROM \"users\" WHERE \"status\" = 'active'";
    await new SqlDataFetcher(
      sqlContext({ scope: 'filtered_table', sql, dataFormat: 'xlsx', primaryKey: 'id' }),
    ).fetch();
    expect(request().dataFormat).toBe('xlsx');
    expect(request().tables).toEqual([
      {
        tableName: 'users',
        schema: undefined,
        columns: ['id', 'name'],
        source: { type: 'sql', sql },
        primaryKey: 'id',
      },
    ]);
  });

  it('forwards database and schema hints', async () => {
    await new SqlDataFetcher(
      sqlContext({ database: 'appdb', schema: 'public' }),
    ).fetch();
    expect(request().database).toBe('appdb');
    expect(request().schema).toBe('public');
    expect(request().tables[0]!.schema).toBe('public');
  });

  it('falls back to an empty column list when the context carries no columns', async () => {
    const { columnNames: _dropped, columns: _alsoDropped, ...ctx } = sqlContext();
    await new SqlDataFetcher(ctx).fetch();
    expect(request().tables[0]!.columns).toEqual([]);
  });

  it('carries mode and ddl when the context carries them (structure modes)', async () => {
    await new SqlDataFetcher(
      sqlContext({
        mode: 'structure_only',
        ddl: 'CREATE TABLE users (id INT);',
        dataFormat: 'sql_insert',
      }),
    ).fetch();
    expect(request().mode).toBe('structure_only');
    expect(request().tables[0]!.ddl).toBe('CREATE TABLE users (id INT);');
  });

  it('forwards progress events with rowsTotal unknown, then unsubscribes', async () => {
    exportTablesMock.mockImplementation(async () => {
      progressHandlerSlot.current?.({ table: 'users', rowsWritten: 7 });
      progressHandlerSlot.current?.({ table: 'users', rowsWritten: 9 });
      return { Saved: 3 };
    });
    const fetcher = new SqlDataFetcher(sqlContext());
    const seen: ExportProgress[] = [];
    fetcher.onProgress((p) => seen.push(p));
    await expect(fetcher.fetch()).resolves.toBe('saved');
    expect(seen).toHaveLength(2);
    expect(seen[0]).toEqual({ table: 'users', rowsWritten: 7 });
    expect(seen[1]).toEqual({ table: 'users', rowsWritten: 9 });
    // Streaming progress never knows the total — the key must be absent,
    // not merely undefined (toEqual would not catch that).
    expect('rowsTotal' in seen[0]!).toBe(false);
    expect('rowsTotal' in seen[1]!).toBe(false);
    expect(subscribeProgressMock).toHaveBeenCalledTimes(1);
    expect(unlistenMock).toHaveBeenCalledTimes(1);
  });

  it('unsubscribes even when the export is cancelled', async () => {
    exportTablesMock.mockResolvedValue({ Cancelled: null });
    await expect(new SqlDataFetcher(sqlContext()).fetch()).resolves.toBe('cancelled');
    expect(unlistenMock).toHaveBeenCalledTimes(1);
  });

  it('reports cancelled when the Rust command returns Cancelled or null', async () => {
    exportTablesMock.mockResolvedValue({ Cancelled: null });
    await expect(new SqlDataFetcher(sqlContext()).fetch()).resolves.toBe('cancelled');
    exportTablesMock.mockResolvedValue(null);
    await expect(new SqlDataFetcher(sqlContext()).fetch()).resolves.toBe('cancelled');
  });

  it('throws Missing connection without a session and touches nothing', async () => {
    await expect(
      new SqlDataFetcher(sqlContext({ dbSessionId: '' })).fetch(),
    ).rejects.toThrow('Missing connection');
    expect(exportTablesMock).not.toHaveBeenCalled();
    expect(subscribeProgressMock).not.toHaveBeenCalled();
  });

  it('stops emitting to a subscriber after unsubscribe', async () => {
    exportTablesMock.mockImplementation(async () => {
      progressHandlerSlot.current?.({ table: 'users', rowsWritten: 1 });
      return { Saved: 3 };
    });
    const fetcher = new SqlDataFetcher(sqlContext());
    const kept: ExportProgress[] = [];
    const dropped: ExportProgress[] = [];
    const unsubscribe = fetcher.onProgress((p) => dropped.push(p));
    fetcher.onProgress((p) => kept.push(p));
    unsubscribe();
    await expect(fetcher.fetch()).resolves.toBe('saved');
    expect(dropped).toEqual([]);
    expect(kept).toEqual([{ table: 'users', rowsWritten: 1 }]);
  });
});
