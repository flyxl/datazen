import { expect } from '@wdio/globals';
import { invokeBackend } from '../helpers.js';
import type { DataSyncTableResult, DataSyncSqlStatement } from '../../packages/frontend/src/commands/sync.js';

/** Isolated relational IPC journey. Credentials never included in assertion messages. */
export function syncWaveOneJourney(databaseType: 'postgresql' | 'mysql') {
  describe(`[tester] ${databaseType} canonical sync projection`, function () {
    this.timeout(180_000);

    it('round-trips binary values and applies only selected changes after a failed plan rolls back', async () => {
      const prefix = databaseType === 'mysql' ? 'E2E_MYSQL' : 'E2E_PG';
      const host = process.env[`${prefix}_HOST`] || '127.0.0.1';
      if (!['localhost', '127.0.0.1', '::1'].includes(host))
        throw new Error('test requires loopback database');
      const sessions: string[] = [];
      const sourceDatabase =
        databaseType === 'postgresql' ? 'datazen_sync_src' : 'datazen_sync_mysql_src';
      const targetDatabase =
        databaseType === 'postgresql' ? 'datazen_sync_tgt' : 'datazen_sync_mysql_tgt';
      for (const [side, database] of [
        ['src', sourceDatabase],
        ['tgt', targetDatabase],
      ]) {
        const id = `tester_sync_${databaseType}_${side}`;
        await invokeBackend('save_connection', {
          config: {
            id,
            name: id,
            databaseType,
            host,
            port: Number(process.env[`${prefix}_PORT`]) || (databaseType === 'mysql' ? 3306 : 5432),
            database,
            username:
              process.env[`${prefix}_USER`] || (databaseType === 'mysql' ? 'root' : 'postgres'),
            password: process.env[`${prefix}_PASSWORD`] || '',
            sslMode: 'disable',
          },
        });
        sessions.push(await invokeBackend<string>('connect', { connectionId: id }));
      }
      const [sourceDbSessionId, targetDbSessionId] = sessions;
      const settings = await invokeBackend<Record<string, unknown>>('get_host_settings');
      await invokeBackend('save_host_settings', { settings: { ...settings, safeMode: false } });
      const sql = (dbSessionId: string, query: string) =>
        invokeBackend<{ data: { results: { rows: unknown[][] }[] } }>('execute_driver_command', {
          request: { dbSessionId, command: 'query', input: { sql: query } },
        });
      for (const session of sessions)
        await sql(session, 'DROP TABLE IF EXISTS tester_sync_projection');
      const binaryType = databaseType === 'postgresql' ? 'BYTEA' : 'LONGBLOB';
      const bytes = (hex: string) =>
        databaseType === 'postgresql' ? `decode('${hex}', 'hex')` : `UNHEX('${hex}')`;
      const hexExpr =
        databaseType === 'postgresql' ? "encode(payload, 'hex')" : 'LOWER(HEX(payload))';
      await sql(
        sourceDbSessionId,
        `CREATE TABLE tester_sync_projection (id INT PRIMARY KEY, a INT, b INT, payload ${binaryType})`,
      );
      // Deliberately reorder the target columns to catch positional projection bugs.
      await sql(
        targetDbSessionId,
        `CREATE TABLE tester_sync_projection (payload ${binaryType}, b INT, id INT PRIMARY KEY, a INT)`,
      );
      await sql(
        sourceDbSessionId,
        `INSERT INTO tester_sync_projection (id,a,b,payload) VALUES ` +
          `(1,10,20,${bytes('00fffe')}),(2,30,40,${bytes('8000ff')}),(3,50,60,${bytes('0102fe')})`,
      );
      await sql(
        targetDbSessionId,
        `INSERT INTO tester_sync_projection (id,a,b,payload) VALUES ` +
          `(1,9,20,${bytes('dead')}),(3,0,60,${bytes('cafe')})`,
      );
      const args = {
        sourceDbSessionId,
        targetDbSessionId,
        sourceDatabase,
        targetDatabase,
        tables: ['tester_sync_projection'],
        options: { insert: true, update: true, delete: false, batchSize: 1 },
      };
      const compared = await invokeBackend<DataSyncTableResult[]>('compare_data_sync', args);
      const table = compared.find((r) => r.sourceTable === 'tester_sync_projection');
      expect(table?.columns).toEqual(['id', 'a', 'b', 'payload']);
      expect(table?.rows).toHaveLength(3);
      for (const row of table?.rows || []) if (row.key[0] === 3) row.selected = false;
      const statements = await invokeBackend<DataSyncSqlStatement[]>('generate_data_sync_sql', {
        ...args,
        tables: compared,
      });
      expect(statements).toHaveLength(2);
      expect(statements.map((statement) => statement.rowKey)).toEqual([[1], [2]]);
      const binaryStatement = statements.find((statement) =>
        statement.parameters.some(
          (parameter) => Array.isArray(parameter) && parameter.join(',') === '0,255,254',
        ),
      );
      expect(binaryStatement).toBeDefined();
      const driverBinaryLiteral = databaseType === 'postgresql' ? "'\\x00fffe'" : "X'00fffe'";
      expect(binaryStatement?.previewSql).toContain(driverBinaryLiteral);
      expect(binaryStatement?.previewSql).not.toContain('\u0000');
      expect(binaryStatement?.previewSql).not.toContain('\ufffd');
      await expect(invokeBackend('apply_data_sync', args)).rejects.toThrow(/legacy apply/);

      const selectTarget = () =>
        sql(
          targetDbSessionId,
          `SELECT id,a,b,${hexExpr} AS payload_hex FROM tester_sync_projection ORDER BY id`,
        );
      const beforeWrite = await selectTarget();
      expect(beforeWrite.data.results[0].rows).toEqual([
        [1, 9, 20, 'dead'],
        [3, 0, 60, 'cafe'],
      ]);

      // The first valid statement may run, but the invalid second statement must roll it back.
      const failingStatements = statements.map((statement, index) =>
        index === statements.length - 1
          ? {
              ...statement,
              sql:
                databaseType === 'postgresql'
                  ? 'INSERT INTO "__datazen_sync_missing_table__" ("id") VALUES ($1)'
                  : 'INSERT INTO `__datazen_sync_missing_table__` (`id`) VALUES (?)',
              parameters: [999],
            }
          : statement,
      );
      await expect(
        invokeBackend('execute_data_sync', {
          targetDbSessionId,
          targetDatabase,
          statements: failingStatements,
        }),
      ).rejects.toThrow();
      const afterFailure = await selectTarget();
      expect(afterFailure.data.results[0].rows).toEqual(beforeWrite.data.results[0].rows);

      await invokeBackend('execute_data_sync', { targetDbSessionId, targetDatabase, statements });
      const result = await selectTarget();
      expect(result.data.results[0].rows).toEqual([
        [1, 10, 20, '00fffe'],
        [2, 30, 40, '8000ff'],
        [3, 0, 60, 'cafe'],
      ]);
      const after = await invokeBackend<DataSyncTableResult[]>('compare_data_sync', args);
      expect(after[0].unchangedCount).toBe(2);
      expect(after[0].rows?.map((r) => r.key)).toEqual([[3]]);
      for (const session of sessions) await sql(session, 'DROP TABLE tester_sync_projection');
      await invokeBackend('save_host_settings', { settings });
    });
  });
}
