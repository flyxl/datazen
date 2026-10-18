/** Real desktop Workflow Job composition; driver fault/lock evidence lives in
 * tests/workflow_transaction_evidence.rs. No acknowledgement-loss injection. */
import { browser, expect } from '@wdio/globals';
import {
  E2E_PG_CONNECTION_ID,
  connectBackend,
  disconnectBackend,
  invokeBackend,
  queryScalar,
  withSafeModeOff,
  type QueryResultPayload,
} from '../../../../e2e/helpers.js';

type Run = { success: boolean; steps: Array<{ stepId: string; status: string }> };

describe('postgres transactionBlock production journey', () => {
  it('commits once and rolls back all earlier DML when a later statement fails', async function () {
    this.timeout(120_000);
    await browser.setTimeout({ script: 120_000 });
    const suffix = `${Date.now()}_${Math.floor(Math.random() * 1_000_000)}`;
    const connectionId = `wf_tx_${suffix}`;
    const table = `wf_tx_${suffix}`;
    const workflows: string[] = [];
    let session: string | undefined;
    // Copy the seeded disposable profile inside the browser only. Credentials
    // are never returned to the runner or printed. Bind the proof namespace
    // explicitly without mutating the shared seeded profile.
    const database = await browser.executeAsync(
      (
        source: string,
        id: string,
        done: (result: { database?: string; failed?: boolean }) => void,
      ) => {
        const api = (
          window as unknown as {
            __TAURI_INTERNALS__: { invoke: (command: string, args?: unknown) => Promise<unknown> };
          }
        ).__TAURI_INTERNALS__;
        void (async () => {
          const rows = (await api.invoke('get_connections')) as Array<Record<string, unknown>>;
          const original = rows.find(
            (row) => row.id === source && row.databaseType === 'postgresql',
          );
          if (!original || typeof original.database !== 'string')
            throw new Error('fixture missing');
          await api.invoke('save_connection', {
            config: { ...original, id, name: id, schema: 'public' },
          });
          done({ database: original.database });
        })().catch(() => done({ failed: true }));
      },
      E2E_PG_CONNECTION_ID,
      connectionId,
    );
    if (!database.database || database.failed)
      throw new Error('Required seeded PostgreSQL fixture unavailable');
    try {
      session = await connectBackend(connectionId);
      const sql = (text: string) =>
        invokeBackend<QueryResultPayload>('execute_query', {
          dbSessionId: session,
          database: database.database,
          sql: text,
        });
      await sql(`CREATE TABLE public.${table}(id INTEGER, label TEXT)`);
      for (const fails of [false, true]) {
        const id = `${connectionId}_${fails ? 'rollback' : 'commit'}`;
        workflows.push(id);
        await invokeBackend('workflow_save', {
          workflow: {
            id,
            name: id,
            description: 'PostgreSQL transaction production journey',
            version: '2',
            variables: [],
            connection: connectionId,
            database: database.database,
            steps: [],
            blocks: [
              {
                type: 'transactionBlock',
                id: 'atomic',
                steps: [
                  {
                    type: 'command',
                    id: 'write',
                    command: 'execute',
                    input: {
                      sql: `INSERT INTO ${table}(id, label) VALUES (${fails ? 2 : 1}, 'marker')`,
                    },
                  },
                  ...(fails
                    ? [
                        {
                          type: 'command',
                          id: 'fail',
                          command: 'execute',
                          input: {
                            sql: `INSERT INTO ${table}(id) VALUES ('invalid integer')`,
                          },
                        },
                      ]
                    : []),
                ],
              },
            ],
          },
        });
        const run = await invokeBackend<Run>('workflow_execute', {
          workflowId: id,
          variables: {},
          connectionId,
        });
        expect(run.success).toBe(!fails);
        expect(run.steps.find((step) => step.stepId === 'write')?.status).toBe('success');
        if (fails) expect(run.steps.find((step) => step.stepId === 'fail')?.status).toBe('failed');
        // Separate pooled observer sees exactly the committed row, including
        // after the failed block whose first INSERT really ran.
        expect(queryScalar(await sql(`SELECT count(*) FROM public.${table}`))).toBe(1);
        expect(queryScalar(await sql(`SELECT id FROM public.${table}`))).toBe(1);
      }
    } finally {
      try {
        if (session)
          await withSafeModeOff(() =>
            invokeBackend('execute_query', {
              dbSessionId: session,
              database: database.database,
              sql: `DROP TABLE IF EXISTS public.${table}`,
            }),
          );
      } finally {
        if (session) await disconnectBackend(session);
        for (const workflowId of workflows) await invokeBackend('workflow_delete', { workflowId });
        await invokeBackend('delete_connection', { id: connectionId });
      }
    }
  });
});
