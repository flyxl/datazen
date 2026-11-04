/** [tester] Run against the exact track WebDriver binary and isolated app data.
 * Uses only dedicated dz_mig_0910_schema_* databases, unique tables, no shared reset.
 */
import { expect } from '@wdio/globals';
import { disconnectBackend, invokeBackend, withSafeModeOff } from '../../../../e2e/helpers.js';

type SchemaDiffPlan = { statements: unknown[] };
type SchemaDiffDeployResult = { status: string };

describe('[tester] PostgreSQL primary-key removal and nullable column journey', () => {
  it('deploys reviewed operations in a valid order and rejects replay', async () => {
    const host = process.env.E2E_PG_HOST ?? '127.0.0.1';
    expect(['127.0.0.1', 'localhost', '::1']).toContain(host);
    const stamp = Date.now().toString(36);
    const table = `tester_schema_pk_${stamp}`;
    const sessions: string[] = [];
    const ids: string[] = [];
    try {
      for (const side of ['src', 'tgt']) {
        const id = `e2e_schema_pk_${side}_${stamp}`;
        ids.push(id);
        await invokeBackend('save_connection', {
          config: {
            id,
            name: id,
            databaseType: 'postgresql',
            host,
            port: Number(process.env.E2E_PG_PORT ?? 5432),
            username: process.env.E2E_PG_USER,
            password: process.env.E2E_PG_PASSWORD ?? '',
            database: `dz_mig_0910_schema_${side}`,
            sslMode: 'disable',
          },
        });
        const session = await invokeBackend<string>('connect', { connectionId: id });
        sessions.push(session);
        await withSafeModeOff(() =>
          invokeBackend('execute_query', {
            dbSessionId: session,
            sql: `CREATE TABLE ${table} (id integer ${side === 'tgt' ? 'PRIMARY KEY' : ''})`,
          }),
        );
      }
      const plan = await invokeBackend<SchemaDiffPlan>('prepare_schema_diff_plan', {
        sourceDbSessionId: sessions[0],
        targetDbSessionId: sessions[1],
        tableNames: [table],
        allowDestructive: true,
        includeIndexes: true,
      });
      const args = {
        targetDbSessionId: sessions[1],
        plan,
        useTransaction: true,
        confirmDestructive: 'DEPLOY',
      };
      const result = await invokeBackend<SchemaDiffDeployResult>(
        'execute_schema_diff_deploy',
        args,
      );
      expect(result.status).toBe('committed');
      await expect(invokeBackend('execute_schema_diff_deploy', args)).rejects.toThrow();
      const after = await invokeBackend<SchemaDiffPlan>('prepare_schema_diff_plan', {
        sourceDbSessionId: sessions[0],
        targetDbSessionId: sessions[1],
        tableNames: [table],
        allowDestructive: true,
        includeIndexes: true,
      });
      expect(after.statements).toHaveLength(0);
    } finally {
      for (const session of sessions) {
        try {
          await withSafeModeOff(() =>
            invokeBackend('execute_query', {
              dbSessionId: session,
              sql: `DROP TABLE IF EXISTS ${table}`,
            }),
          );
        } finally {
          await disconnectBackend(session);
        }
      }
      for (const id of ids) await invokeBackend('delete_connection', { id });
    }
  });
});
