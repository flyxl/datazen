import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import {
  exportSingleTableStructureViaRust,
  exportSingleTableViaRust,
  runBatchExportJob,
  type BatchExportTableInput,
  type RunBatchExportJobOptions,
  type SingleTableExportOptions,
  type SingleTableStructureExportOptions,
} from '../batchExportJob';
import type { ExportTablesRequest } from '../../commands/file';
import { configurePlatform, clearPlatform } from '../../platform/contracts';

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
      // The fetcher subscribes to batch-export-progress by default; the
      // stub keeps these tests off the (unbound here) platform event bus.
      subscribeProgress: async () => () => {},
      ...extra,
    };
  }

  it('sends a single-table request with a table source for entire tables', async () => {
    const result = await exportSingleTableViaRust(base());
    expect(result).toEqual({ status: 'saved' });
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
    await expect(exportSingleTableViaRust(base())).resolves.toEqual({ status: 'cancelled' });
    exportTables.mockResolvedValue(null);
    await expect(exportSingleTableViaRust(base())).resolves.toEqual({ status: 'cancelled' });
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

describe('default tables exporter (platform binding)', () => {
  const exportTables = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
    exportTables.mockResolvedValue({ Saved: 1 });
    configurePlatform({
      services: {} as never,
      windows: {} as never,
      events: {
        emitCrossWindow: async () => {},
        listenCrossWindow: async () => () => {},
      },
      tablesExporter: { exportTables },
    });
  });

  afterEach(() => {
    clearPlatform();
  });

  it('runBatchExportJob uses the bound exporter when none is injected', async () => {
    const loadTableExportData = vi.fn().mockResolvedValue(users);
    const result = await runBatchExportJob({
      tableNames: ['users'],
      mode: 'data_only',
      dataFormat: 'csv',
      outputMode: 'single',
      dbSessionId: 'c1',
      loadTableExportData,
    });
    expect(result).toEqual({ status: 'saved' });
    expect(exportTables).toHaveBeenCalledTimes(1);
  });

  it('exportSingleTableStructureViaRust uses the bound exporter when none is injected', async () => {
    const result = await exportSingleTableStructureViaRust({
      dbSessionId: 'c1',
      tableName: 'users',
      ddl: 'CREATE TABLE users (id INT);',
    });
    expect(result).toBe('saved');
    expect(exportTables).toHaveBeenCalledTimes(1);
  });
});

describe('ExportTablesRequest IPC contract guard (BUG-006)', () => {
  // Mirrors `ExportTablesRequest` in src-tauri/src/commands/export.rs:
  // #[serde(rename_all = "camelCase")] over
  //   db_session_id / cancel_token / database_type / database / schema /
  //   mode / data_format / output_mode / tables.
  // `cancel_token` is optional and carried only by SQL-path requests
  // (the per-run cancellation token); provided exports never reach Rust.
  // serde has NO aliases and NO deny_unknown_fields: an unexpected key is
  // silently dropped and a missing required key is a hard rejection — so the
  // wire key set must match exactly, neither more nor less.
  const BACKEND_WIRE_KEYS = [
    'dbSessionId',
    'cancelToken',
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
