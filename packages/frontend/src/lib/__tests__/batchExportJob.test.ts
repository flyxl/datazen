import { describe, expect, it, vi, beforeEach } from 'vitest';
import {
  exportProvidedRowsViaRust,
  exportSingleTableStructureViaRust,
  exportSingleTableViaRust,
  runBatchExportJob,
  toExportWireValue,
  type BatchExportTableInput,
  type ProvidedRowsExportOptions,
  type RunBatchExportJobOptions,
  type SingleTableExportOptions,
  type SingleTableStructureExportOptions,
} from '../batchExportJob';
import type { ExportTablesRequest } from '../../commands/file';

const users: BatchExportTableInput = {
  tableName: 'users',
  ddl: 'CREATE TABLE users (id INT);',
  columns: [{ name: 'id' }, { name: 'name' }],
  rows: [],
};

const orders: BatchExportTableInput = {
  tableName: 'orders',
  ddl: 'CREATE TABLE orders (id INT);',
  columns: [{ name: 'id' }],
  rows: [],
};

describe('runBatchExportJob', () => {
  const loadTableExportData = vi.fn();
  const exportTables = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
    exportTables.mockResolvedValue({ Saved: 42 });
    loadTableExportData.mockImplementation(async (name: string) => {
      if (name === 'users') return users;
      if (name === 'orders') return orders;
      throw new Error(`unknown table ${name}`);
    });
  });

  function base(extra: Partial<RunBatchExportJobOptions> = {}): RunBatchExportJobOptions {
    return {
      tableNames: ['users'],
      mode: 'data_and_structure',
      dataFormat: 'csv',
      outputMode: 'zip',
      dbSessionId: 'c1',
      databaseType: 'postgres',
      loadTableExportData,
      exportTables,
      ...extra,
    };
  }

  it('throws when no tables selected', async () => {
    await expect(runBatchExportJob(base({ tableNames: [] }))).rejects.toThrow('no_tables_selected');
    expect(loadTableExportData).not.toHaveBeenCalled();
  });

  it('throws when dbSessionId is missing', async () => {
    await expect(runBatchExportJob(base({ dbSessionId: undefined }))).rejects.toThrow(
      'Missing connection',
    );
  });

  it('loads table metadata in order and sends a Rust-side request', async () => {
    const onProgress = vi.fn();
    const result = await runBatchExportJob(
      base({
        tableNames: ['users', 'orders'],
        mode: 'data_and_structure',
        dataFormat: 'sql_insert',
        outputMode: 'single',
        onProgress,
      }),
    );

    expect(result).toEqual({ status: 'saved' });
    expect(loadTableExportData.mock.calls.map((c) => c[0])).toEqual(['users', 'orders']);
    expect(onProgress).toHaveBeenCalledWith({ current: 1, total: 2, tableName: 'users' });
    expect(onProgress).toHaveBeenCalledWith({ current: 2, total: 2, tableName: 'orders' });

    const request = exportTables.mock.calls[0]![0] as ExportTablesRequest;
    expect(request.dbSessionId).toBe('c1');
    expect(request.databaseType).toBe('postgres');
    expect(request.mode).toBe('data_and_structure');
    expect(request.dataFormat).toBe('sql_insert');
    expect(request.outputMode).toBe('single');
    expect(request.tables).toEqual([
      { tableName: 'users', columns: ['id', 'name'], ddl: 'CREATE TABLE users (id INT);' },
      { tableName: 'orders', columns: ['id'], ddl: 'CREATE TABLE orders (id INT);' },
    ]);
    expect(exportTables).toHaveBeenCalledTimes(1);
  });

  it('reports cancelled when the Rust command returns Cancelled', async () => {
    exportTables.mockResolvedValue({ Cancelled: null });
    const result = await runBatchExportJob(base());
    expect(result).toEqual({ status: 'cancelled' });
  });

  it('propagates load errors', async () => {
    loadTableExportData.mockRejectedValue(new Error('ddl failed'));
    await expect(runBatchExportJob(base())).rejects.toThrow('ddl failed');
  });
});

describe('exportSingleTableViaRust (item 3b)', () => {
  const exportTables = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
    exportTables.mockResolvedValue({ Saved: 3 });
  });

  function base(extra: Partial<SingleTableExportOptions> = {}): SingleTableExportOptions {
    return {
      dbSessionId: 'c1',
      databaseType: 'postgres',
      tableName: 'users',
      columns: ['id', 'name'],
      dataFormat: 'csv',
      exportTables,
      ...extra,
    };
  }

  it('sends a single-table request with a table source for entire tables', async () => {
    const result = await exportSingleTableViaRust(base());
    expect(result).toBe('saved');
    expect(exportTables).toHaveBeenCalledTimes(1);
    const request = exportTables.mock.calls[0]![0] as ExportTablesRequest;
    expect(request.mode).toBe('data_only');
    expect(request.outputMode).toBe('single');
    expect(request.dataFormat).toBe('csv');
    expect(request.tables).toEqual([
      {
        tableName: 'users',
        columns: ['id', 'name'],
        source: { type: 'table' },
        primaryKey: undefined,
      },
    ]);
  });

  it('sends a SQL source carrying the caller-built statement for filtered tables', async () => {
    const sql = 'SELECT "id" FROM "users" WHERE "status" = \'active\'';
    await exportSingleTableViaRust(base({ sql, dataFormat: 'xlsx', primaryKey: 'id' }));
    const request = exportTables.mock.calls[0]![0] as ExportTablesRequest;
    expect(request.dataFormat).toBe('xlsx');
    expect(request.tables).toEqual([
      {
        tableName: 'users',
        columns: ['id', 'name'],
        source: { type: 'sql', sql },
        primaryKey: 'id',
      },
    ]);
  });

  it('reports cancelled when the Rust command returns Cancelled or null', async () => {
    exportTables.mockResolvedValue({ Cancelled: null });
    await expect(exportSingleTableViaRust(base())).resolves.toBe('cancelled');
    exportTables.mockResolvedValue(null);
    await expect(exportSingleTableViaRust(base())).resolves.toBe('cancelled');
  });

  it('throws when dbSessionId is missing', async () => {
    await expect(exportSingleTableViaRust(base({ dbSessionId: '' }))).rejects.toThrow(
      'Missing connection',
    );
    expect(exportTables).not.toHaveBeenCalled();
  });

  it('forwards database and schema hints', async () => {
    await exportSingleTableViaRust(base({ database: 'appdb', schema: 'public' }));
    const request = exportTables.mock.calls[0]![0] as ExportTablesRequest;
    expect(request.database).toBe('appdb');
    expect(request.schema).toBe('public');
  });
});

describe('exportProvidedRowsViaRust (item 3c)', () => {
  const exportTables = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
    exportTables.mockResolvedValue({ Saved: 2 });
  });

  function base(extra: Partial<ProvidedRowsExportOptions> = {}): ProvidedRowsExportOptions {
    return {
      dbSessionId: 'c1',
      databaseType: 'postgres',
      tableName: 'users',
      columns: [
        { name: 'id', dataType: 'int', nullable: false },
        { name: 'name', dataType: 'text', nullable: true },
      ],
      rows: [
        [1, 'a'],
        [2, null],
      ],
      dataFormat: 'csv',
      exportTables,
      ...extra,
    };
  }

  it('sends rows + schema as a provided source, never a SQL translation', async () => {
    const result = await exportProvidedRowsViaRust(base({ dataFormat: 'xlsx' }));
    expect(result).toBe('saved');
    expect(exportTables).toHaveBeenCalledTimes(1);
    const request = exportTables.mock.calls[0]![0] as ExportTablesRequest;
    expect(request.mode).toBe('data_only');
    expect(request.dataFormat).toBe('xlsx');
    expect(request.outputMode).toBe('single');
    expect(request.tables).toEqual([
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
            [1, 'a'],
            [2, null],
          ],
        },
        primaryKey: undefined,
      },
    ]);
  });

  it('maps values onto the wire Value shape', async () => {
    await exportProvidedRowsViaRust(
      base({
        columns: [
          { name: 'b', dataType: 'bytea', nullable: true },
          { name: 'n', dataType: 'float8', nullable: true },
          { name: 'j', dataType: 'jsonb', nullable: true },
          { name: 'u', dataType: 'text', nullable: true },
        ],
        rows: [[new Uint8Array([0, 255]), Number.NaN, { k: 1 }, undefined]],
        primaryKey: 'b',
      }),
    );
    const request = exportTables.mock.calls[0]![0] as ExportTablesRequest;
    const source = request.tables[0]!.source as {
      type: string;
      rows: unknown[][];
    };
    expect(source.type).toBe('provided');
    expect(source.rows).toEqual([[[0, 255], null, { k: 1 }, null]]);
    expect(request.tables[0]!.primaryKey).toBe('b');
  });

  it('reports cancelled and throws without a session', async () => {
    exportTables.mockResolvedValue({ Cancelled: null });
    await expect(exportProvidedRowsViaRust(base())).resolves.toBe('cancelled');
    await expect(exportProvidedRowsViaRust(base({ dbSessionId: '' }))).rejects.toThrow(
      'Missing connection',
    );
  });
});

describe('exportSingleTableStructureViaRust (item 6: G merged into C)', () => {
  const exportTables = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
    exportTables.mockResolvedValue({ Saved: 0 });
  });

  function base(
    extra: Partial<SingleTableStructureExportOptions> = {},
  ): SingleTableStructureExportOptions {
    return {
      dbSessionId: 'c1',
      databaseType: 'mysql',
      database: 'app',
      tableName: 'users',
      ddl: 'CREATE TABLE users (id INT);',
      exportTables,
      ...extra,
    };
  }

  it('sends a structure_only single-table request carrying the caller DDL', async () => {
    const result = await exportSingleTableStructureViaRust(base());
    expect(result).toBe('saved');
    expect(exportTables).toHaveBeenCalledTimes(1);
    const request = exportTables.mock.calls[0]![0] as ExportTablesRequest;
    expect(request.mode).toBe('structure_only');
    expect(request.outputMode).toBe('single');
    // sql_insert is only the `.sql` dialog-extension carrier; the planner
    // never consults the format for structure_only.
    expect(request.dataFormat).toBe('sql_insert');
    expect(request.tables).toEqual([
      {
        tableName: 'users',
        schema: undefined,
        columns: [],
        ddl: 'CREATE TABLE users (id INT);',
      },
    ]);
  });

  it('passes a missing DDL through so the backend emits its placeholder', async () => {
    await exportSingleTableStructureViaRust(base({ ddl: null }));
    const request = exportTables.mock.calls[0]![0] as ExportTablesRequest;
    expect(request.tables[0]!.ddl).toBeUndefined();
  });

  it('reports cancelled when the Rust command returns Cancelled or null', async () => {
    exportTables.mockResolvedValue({ Cancelled: null });
    await expect(exportSingleTableStructureViaRust(base())).resolves.toBe('cancelled');
    exportTables.mockResolvedValue(null);
    await expect(exportSingleTableStructureViaRust(base())).resolves.toBe('cancelled');
  });

  it('throws when dbSessionId is missing', async () => {
    await expect(exportSingleTableStructureViaRust(base({ dbSessionId: '' }))).rejects.toThrow(
      'Missing connection',
    );
    expect(exportTables).not.toHaveBeenCalled();
  });

  it('forwards database and schema hints', async () => {
    await exportSingleTableStructureViaRust(base({ database: 'appdb', schema: 'public' }));
    const request = exportTables.mock.calls[0]![0] as ExportTablesRequest;
    expect(request.database).toBe('appdb');
    expect(request.schema).toBe('public');
    expect(request.tables[0]!.schema).toBe('public');
  });
});

describe('toExportWireValue', () => {
  it('passes scalars through and maps edge cases', () => {
    expect(toExportWireValue(null)).toBeNull();
    expect(toExportWireValue(undefined)).toBeNull();
    expect(toExportWireValue(true)).toBe(true);
    expect(toExportWireValue('x')).toBe('x');
    expect(toExportWireValue(1)).toBe(1);
    expect(toExportWireValue(1.5)).toBe(1.5);
    expect(toExportWireValue(Number.NaN)).toBeNull();
    expect(toExportWireValue(Number.POSITIVE_INFINITY)).toBeNull();
    expect(toExportWireValue(10n)).toBe(10);
    expect(toExportWireValue(2n ** 100n)).toBe((2n ** 100n).toString());
    expect(toExportWireValue(new Uint8Array([1, 2]))).toEqual([1, 2]);
    expect(toExportWireValue([1, 2])).toEqual([1, 2]);
    expect(toExportWireValue({ a: 1 })).toEqual({ a: 1 });
  });
});

describe('ExportTablesRequest IPC contract guard (BUG-006)', () => {
  // Mirrors `ExportTablesRequest` in src-tauri/src/commands/export.rs:
  // #[serde(rename_all = "camelCase")] over
  //   db_session_id / database_type / database / schema / mode / data_format /
  //   output_mode / tables.
  // serde has NO aliases and NO deny_unknown_fields: an unexpected key is
  // silently dropped and a missing required key is a hard rejection — so the
  // wire key set must match exactly, neither more nor less.
  const BACKEND_WIRE_KEYS = [
    'dbSessionId',
    'databaseType',
    'database',
    'schema',
    'mode',
    'dataFormat',
    'outputMode',
    'tables',
  ].sort();

  it('builds a payload whose key set matches the backend contract', async () => {
    const loadTableExportData = vi.fn().mockResolvedValue(users);
    const exportTables = vi.fn().mockResolvedValue({ Saved: 1 });
    await runBatchExportJob({
      tableNames: ['users'],
      mode: 'data_and_structure',
      dataFormat: 'sql_insert',
      outputMode: 'single',
      dbSessionId: 'live-session-1',
      databaseType: 'postgres',
      database: 'appdb',
      schema: 'public',
      loadTableExportData,
      exportTables,
    });

    expect(exportTables).toHaveBeenCalledTimes(1);
    const request = exportTables.mock.calls[0]![0] as ExportTablesRequest;
    expect(Object.keys(request).sort()).toEqual(BACKEND_WIRE_KEYS);
  });

  it('never emits the retired connectionId key', async () => {
    const loadTableExportData = vi.fn().mockResolvedValue(users);
    const exportTables = vi.fn().mockResolvedValue({ Saved: 1 });
    await runBatchExportJob({
      tableNames: ['users'],
      mode: 'data_only',
      dataFormat: 'csv',
      outputMode: 'zip',
      dbSessionId: 'live-session-1',
      databaseType: undefined,
      loadTableExportData,
      exportTables,
    });

    const request = exportTables.mock.calls[0]![0] as Record<string, unknown>;
    expect(request).not.toHaveProperty('connectionId');
    expect(request.dbSessionId).toBe('live-session-1');
  });

  it('statically pins the interface field name to dbSessionId', () => {
    // Compile-time anchor duplicated at runtime: renaming the field back to
    // `connectionId` must fail this suite even if mocks stay loose.
    type ExpectedShape = { dbSessionId: string } & Record<string, unknown>;
    const sample: ExpectedShape = { dbSessionId: 'x' };
    // If ExportTablesRequest gains/loses the dbSessionId field, this
    // assignment stops compiling.
    const typed: ExportTablesRequest = {
      ...sample,
      databaseType: null,
      mode: 'data_only',
      dataFormat: 'csv',
      outputMode: 'zip',
      tables: [],
    };
    expect(typed.dbSessionId).toBeDefined();
    expect(Object.keys(typed)).toContain('dbSessionId');
  });
});
