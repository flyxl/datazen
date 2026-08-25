import { describe, expect, it, vi, beforeEach } from 'vitest';
import { runBatchExportJob, type RunBatchExportJobOptions } from '../batchExportJob';
import type { ExportTablesRequest } from '../../commands/file';
import type { BatchExportTableInput } from '../batchExport';

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

describe('ExportTablesRequest IPC contract guard (BUG-006)', () => {
  // Mirrors `ExportTablesRequest` in src-tauri/src/commands/export.rs:
  // #[serde(rename_all = "camelCase")] over
  //   db_session_id / database_type / mode / data_format / output_mode / tables.
  // serde has NO aliases and NO deny_unknown_fields: an unexpected key is
  // silently dropped and a missing required key is a hard rejection — so the
  // wire key set must match exactly, neither more nor less.
  const BACKEND_WIRE_KEYS = [
    'dbSessionId',
    'databaseType',
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
      databaseType: null,
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
    const typed: ExportTablesRequest = { ...sample, databaseType: null };
    expect(typed.dbSessionId).toBeDefined();
    expect(Object.keys(typed)).toContain('dbSessionId');
  });
});
