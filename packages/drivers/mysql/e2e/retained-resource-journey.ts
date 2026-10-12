import { browser, expect } from '@wdio/globals';
import { invokeBackend, withSafeModeOff } from '../../../../e2e/helpers.js';
import { withMysqlProfile } from './mysql-profile-fixture.js';
type Handle = { dbSessionId: string; runtimeEpoch: string };
type Session = { handle: Handle; contextRevision: string; observedContext: { transactionState: string } };
type Execution = { state: string; artifactIds: string[]; errorCode: string | null };

describe('[tester] mysql retained resource journey', () => {
  it('retains identity, temp schema, typed rows, stream and rollback', async () => {
    await withMysqlProfile(async (profile) => {
      const identity = await invokeBackend<{ clientInstanceId: string }>('get_platform_identity');
      const token = await invokeBackend<{ idempotencyKey: string }>('issue_submission_token', { request: { operation: 'openSession' } });
      const opened = await invokeBackend<{ session: Session }>('open_session', { request: {
        initialTarget: { connectionId: profile.id, namespace: { database: profile.database, catalog: null, schema: null, path: [] }, object: null },
        owner: { kind: 'editor', clientInstanceId: identity.clientInstanceId, editorSessionId: `driver-verify-${Date.now()}` }, idempotencyKey: token.idempotencyKey,
      } });
      const handle = opened.session.handle;
      async function run(sql: string, boundParameters?: unknown[], command = 'query', extraInput: Record<string, unknown> = {}, expectedState = 'succeeded', cancel = false): Promise<{ rows: unknown[][]; outputs: unknown[]; execution: Execution }> {
        const session = await invokeBackend<Session>('get_session', { request: { handle } });
        const issued = await invokeBackend<{ idempotencyKey: string }>('issue_submission_token', { request: { operation: 'executeInSession', handle } });
        const receipt = await invokeBackend<{ executionId: string }>('execute_in_session', { request: { handle, expectedContextRevision: session.contextRevision, call: { command, input: { sql, ...extraInput, ...(boundParameters ? { boundParameters } : {}) } }, idempotencyKey: issued.idempotencyKey } });
        let execution: Execution = { state: 'accepted', artifactIds: [], errorCode: null };
        if (cancel) {
          await browser.waitUntil(async () => (await invokeBackend<Execution>('get_execution', { request: { executionId: receipt.executionId } })).state === 'running', { timeout: 5000, interval: 50 });
          const cancellation = await invokeBackend<{ idempotencyKey: string; expiresAt: string }>('issue_submission_token', { request: { operation: 'cancelExecution', handle } });
          const cancelled = await invokeBackend<{ disposition: string }>('cancel_execution', { request: { executionId: receipt.executionId, submissionToken: cancellation } });
          expect(cancelled.disposition).toBe('requested');
        }
        await browser.waitUntil(async () => {
          execution = await invokeBackend<Execution>('get_execution', { request: { executionId: receipt.executionId } });
          return ['succeeded', 'failed', 'cancelled'].includes(execution.state);
        }, { timeout: cancel ? 5000 : 20000, interval: 50, timeoutMsg: 'platform execution did not terminate' });
        const rows: unknown[][] = [];
        const outputs: unknown[] = [];
        for (const artifactId of execution.artifactIds) {
          let index = 0;
          while (true) {
            const chunk = await invokeBackend<{ bytes: number[]; publishedChunkCount: string }>('read_artifact_chunk', { request: { artifactId, chunkIndex: String(index) } });
            const payload = JSON.parse(new TextDecoder().decode(new Uint8Array(chunk.bytes))) as { output?: { rows?: unknown[][] }; rows?: unknown[][] };
            rows.push(...(payload.output?.rows ?? payload.rows ?? []));
            if (payload.output) outputs.push(payload.output);
            if (++index >= Number(chunk.publishedChunkCount)) break;
          }
        }
        if (cancel) expect(['failed', 'cancelled']).toContain(execution.state);
        else expect(execution.state).toBe(expectedState);
        return { rows, outputs, execution };
      }
      try {
        await withSafeModeOff(async () => {
          const first = await run('SELECT CONNECTION_ID() AS physical_id');
          const second = await run('SELECT CONNECTION_ID() AS physical_id');
          expect(first.rows[0]?.[0]).toEqual(second.rows[0]?.[0]);
          expect(first.rows).toHaveLength(1);
          const bound = await run('SELECT CAST(? AS SIGNED) AS bound_value', [73]);
          expect(Number(bound.rows[0]?.[0])).toBe(73);
          await run('CREATE TEMPORARY TABLE driver_verify_temp (id integer PRIMARY KEY, label varchar(30))');
          await run("INSERT INTO driver_verify_temp VALUES (1, 'retained')");
          const temp = await run('SELECT id, label FROM driver_verify_temp');
          expect(temp.rows).toEqual([[1, 'retained']]);
          const metadata = await run('', undefined, 'get_table_schema', { relationSql: 'driver_verify_temp', table: 'driver_verify_temp', database: profile.database });
          const object = metadata.outputs.find(value => typeof value === 'object' && value !== null && 'data' in value) as { data?: { resolvedTarget?: { sessionScoped?: boolean }; schema?: { columns?: unknown[] } } } | undefined;
          expect(object?.data?.resolvedTarget?.sessionScoped).toBe(true);
          expect(object?.data?.schema?.columns).toHaveLength(2);
          const many = await run('WITH RECURSIVE seq(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM seq WHERE n < 600) SELECT n FROM seq');
          expect(many.rows).toHaveLength(600);
          await run('BEGIN');
          await run("UPDATE driver_verify_temp SET label = 'rollback' WHERE id = 1");
          await run('ROLLBACK');
          const after = await run('SELECT label FROM driver_verify_temp WHERE id = 1');
          expect(after.rows[0]?.[0]).toBe('retained');
          // A standalone SLEEP returns 1 (success) when KILL QUERY interrupts it.
          // In a table predicate the same interruption raises server error 1317.
          // Keep the terminal-state assertion strict: acceptance alone is not cancellation.
          await run('SELECT id FROM driver_verify_temp WHERE SLEEP(10) = 0', undefined, 'query', {}, 'cancelled', true);
          await run('ROLLBACK');
          const afterCancel = await run('SELECT id FROM driver_verify_temp');
          expect(afterCancel.rows).toEqual([[1]]);
        });
      } finally {
        await invokeBackend('close_session', { request: { handle, mode: 'rollbackAndClose' } });
      }
    });
  });
});
