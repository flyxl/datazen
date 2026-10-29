import { expect } from '@wdio/globals';
import { invokeBackend } from '../helpers.js';
import type {
  DataSyncComparisonPreview,
  DataSyncRowChange,
  DataSyncSelection,
  DataSyncTableResult,
  SyncOptions,
} from '../../packages/frontend/src/commands/sync.js';

type DatabaseType = 'postgresql' | 'mysql';

interface ExecutionResult {
  applied: number;
  rolledBack: boolean;
}

const OPTIONS: SyncOptions = {
  insert: true,
  update: true,
  delete: false,
  matchingStrategy: 'primaryKey',
  batchSize: 1000,
  largeValueMode: 'full',
};

function config(databaseType: DatabaseType, id: string, database: string) {
  const prefix = databaseType === 'postgresql' ? 'E2E_PG' : 'E2E_MYSQL';
  return {
    id,
    name: id,
    databaseType,
    host: process.env[`${prefix}_HOST`] || '127.0.0.1',
    port: Number(process.env[`${prefix}_PORT`]) || (databaseType === 'mysql' ? 3306 : 5432),
    database,
    username: process.env[`${prefix}_USER`] || (databaseType === 'mysql' ? 'root' : 'postgres'),
    password: process.env[`${prefix}_PASSWORD`] || '',
    sslMode: 'disable',
  };
}

async function sql(dbSessionId: string, sqlText: string, database?: string): Promise<void> {
  await invokeBackend('execute_query', {
    dbSessionId,
    sql: sqlText,
    ...(database ? { database } : {}),
  });
}

function selectionFor(
  preview: DataSyncComparisonPreview,
  tableName: string,
  key: number,
): DataSyncSelection {
  const table = preview.tables.find((candidate) => candidate.sourceTable === tableName);
  if (!table) throw new Error(`comparison table ${tableName} is missing`);
  const row = table.rows?.find((candidate) => candidate.key[0] === key);
  if (!row) throw new Error(`comparison row ${tableName}/${key} is missing`);
  return {
    revision: preview.selectionRevision,
    rows: [
      {
        sourceTable: table.sourceTable,
        targetTable: table.targetTable,
        operation: row.operation,
        key: row.key,
      },
    ],
  };
}

function tableFrom(preview: DataSyncComparisonPreview, name: string): DataSyncTableResult {
  const table = preview.tables.find((candidate) => candidate.sourceTable === name);
  if (!table) throw new Error(`comparison table ${name} is missing`);
  return table;
}

function fixtureSql(databaseType: DatabaseType, table: string): string {
  const binaryType = databaseType === 'postgresql' ? 'BYTEA' : 'LONGBLOB';
  const bytes = databaseType === 'postgresql' ? "decode('00fffe', 'hex')" : "UNHEX('00fffe')";
  return [
    `DROP TABLE IF EXISTS ${table}`,
    `CREATE TABLE ${table} (id INT PRIMARY KEY, value INT, payload ${binaryType})`,
    `INSERT INTO ${table} (id, value, payload) VALUES (1, 10, ${bytes}), (2, 20, ${bytes})`,
  ].join('; ');
}

function readSql(databaseType: DatabaseType, table: string): string {
  const hex = databaseType === 'postgresql' ? "encode(payload, 'hex')" : 'LOWER(HEX(payload))';
  return `SELECT id, value, ${hex} AS payload_hex FROM ${table} ORDER BY id`;
}

/**
 * Real immutable-plan IPC journey. It intentionally bypasses the UI so the
 * server-owned request contract and write boundary are directly observable.
 */
export function syncPlanJourney(databaseType: DatabaseType) {
  describe(`[tester] ${databaseType} immutable sync plan`, function () {
    this.timeout(180_000);

    const sourceDatabase =
      databaseType === 'postgresql' ? 'datazen_sync_src' : 'datazen_sync_mysql_src';
    const targetDatabase =
      databaseType === 'postgresql' ? 'datazen_sync_tgt' : 'datazen_sync_mysql_tgt';
    const suffix = `${databaseType}_${Date.now().toString(36)}`;
    const selectedTable = `tester_sync_plan_selected_${suffix}`;
    const staleTable = `tester_sync_plan_stale_${suffix}`;
    const sourceId = `tester_sync_plan_src_${suffix}`;
    const targetId = `tester_sync_plan_tgt_${suffix}`;
    let sourceSessionId = '';
    let targetSessionId = '';

    before(async () => {
      await invokeBackend('save_connection', {
        config: config(databaseType, sourceId, sourceDatabase),
      });
      await invokeBackend('save_connection', {
        config: config(databaseType, targetId, targetDatabase),
      });
      sourceSessionId = await invokeBackend<string>('connect', { connectionId: sourceId });
      targetSessionId = await invokeBackend<string>('connect', { connectionId: targetId });
      const settings = await invokeBackend<Record<string, unknown>>('get_host_settings');
      await invokeBackend('save_host_settings', { settings: { ...settings, safeMode: false } });
      await sql(sourceSessionId, fixtureSql(databaseType, selectedTable));
      await sql(targetSessionId, fixtureSql(databaseType, selectedTable));
      await sql(targetSessionId, `DELETE FROM ${selectedTable} WHERE id = 2`);
      await sql(sourceSessionId, `UPDATE ${selectedTable} SET value = 11 WHERE id = 1`);
      await sql(targetSessionId, `UPDATE ${selectedTable} SET value = 9 WHERE id = 1`);
      await sql(sourceSessionId, fixtureSql(databaseType, staleTable));
      await sql(targetSessionId, fixtureSql(databaseType, staleTable));
      await sql(sourceSessionId, `UPDATE ${staleTable} SET value = 101 WHERE id = 1`);
      await sql(targetSessionId, `UPDATE ${staleTable} SET value = 99 WHERE id = 1`);
    });

    after(async () => {
      for (const [session, table] of [
        [sourceSessionId, selectedTable],
        [targetSessionId, selectedTable],
        [sourceSessionId, staleTable],
        [targetSessionId, staleTable],
      ] as const) {
        if (!session) continue;
        await sql(session, `DROP TABLE IF EXISTS ${table}`).catch(() => undefined);
      }
      for (const session of [sourceSessionId, targetSessionId]) {
        if (session)
          await invokeBackend('disconnect', { dbSessionId: session }).catch(() => undefined);
      }
      await invokeBackend('delete_connection', { id: sourceId }).catch(() => undefined);
      await invokeBackend('delete_connection', { id: targetId }).catch(() => undefined);
    });

    it('keeps selected-only writes inside the immutable server plan', async () => {
      const preview = await invokeBackend<DataSyncComparisonPreview>('compare_data_sync', {
        sourceDbSessionId: sourceSessionId,
        targetDbSessionId: targetSessionId,
        sourceDatabase,
        targetDatabase,
        tables: [selectedTable],
        options: OPTIONS,
      });
      const table = tableFrom(preview, selectedTable);
      expect(table.status).toBe('MATCHED');
      expect(table.rows?.map((row: DataSyncRowChange) => row.key[0])).toEqual([1, 2]);
      const selection = selectionFor(preview, selectedTable, 1);
      const statements = await invokeBackend<Array<{ rowKey: unknown[] }>>(
        'generate_data_sync_sql',
        { planId: preview.planId, selection, options: OPTIONS },
      );
      expect(statements).toHaveLength(1);
      expect(statements[0].rowKey).toEqual([1]);
      const result = await invokeBackend<ExecutionResult>('execute_data_sync', {
        request: { planId: preview.planId, selection, options: OPTIONS, jobId: null },
      });
      expect(result).toMatchObject({ applied: 1, rolledBack: false });
      const rows = await invokeBackend<{ results: { rows: unknown[][] }[] }>('execute_query', {
        dbSessionId: targetSessionId,
        sql: readSql(databaseType, selectedTable),
      });
      expect(rows.results[0].rows).toEqual([[1, 11, '00fffe']]);
    });

    it('rejects a stale target schema before writing', async () => {
      const preview = await invokeBackend<DataSyncComparisonPreview>('compare_data_sync', {
        sourceDbSessionId: sourceSessionId,
        targetDbSessionId: targetSessionId,
        sourceDatabase,
        targetDatabase,
        tables: [staleTable],
        options: OPTIONS,
      });
      const selection = selectionFor(preview, staleTable, 1);
      const before = await invokeBackend<{ results: { rows: unknown[][] }[] }>('execute_query', {
        dbSessionId: targetSessionId,
        sql: readSql(databaseType, staleTable),
      });
      await sql(targetSessionId, `ALTER TABLE ${staleTable} ADD COLUMN drift INT NULL`);
      let error = '';
      try {
        await invokeBackend('execute_data_sync', {
          request: { planId: preview.planId, selection, options: OPTIONS, jobId: null },
        });
      } catch (caught) {
        error = caught instanceof Error ? caught.message : String(caught);
      }
      expect(error).toMatch(/schema|changed|comparison/i);
      const after = await invokeBackend<{ results: { rows: unknown[][] }[] }>('execute_query', {
        dbSessionId: targetSessionId,
        sql: readSql(databaseType, staleTable),
      });
      expect(after.results[0].rows).toEqual(before.results[0].rows);
    });
  });
}
