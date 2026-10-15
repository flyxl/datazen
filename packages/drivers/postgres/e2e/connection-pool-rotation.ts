import { browser, expect } from '@wdio/globals';
import { invokeBackend, withSafeModeOff } from '../../../../e2e/helpers.js';

/**
 * CM-67 (D layer): a real PostgreSQL server observes the pool/rotation
 * contract — the parts that are observable at all once the idle pool is known
 * to be empty by construction (every driver declares
 * `ResetForReuse::Unsupported`, so nothing is ever parked; see
 * docs/architecture/platform/connection-management.md CM-67).
 *
 * What this journey observes on the wire:
 *   * a session's statements land in the session's own target database, and two
 *     sessions on two databases never serve each other's target;
 *   * every session owns a distinct physical backend, and closing it returns the
 *     backend count to the exact previous value (no leak, no over-count);
 *   * a config-revision rotation retires the previous generation: the old
 *     session is refused for reads and executions, but can still be closed with
 *     a confirmed resource release; a session opened afterwards works.
 *
 * "旧 idle 停发并关闭" / "空 pool 元数据受 LRU 上限约束" are intentionally NOT
 * asserted here — no real driver can reach an idle pool, so such an assertion
 * would be vacuously true. They stay H-layer assertions.
 */

type Handle = { dbSessionId: string; runtimeEpoch: string };
type SessionView = {
  handle: Handle;
  configRevision: string;
  contextRevision: string;
  observedContext: { transactionState: string };
};
type Execution = { state: string; artifactIds: string[]; errorCode: string | null };
type ProfileView = { connectionId: string; configRevision: string; enabled: boolean };

const ADMIN_ID = 'e2e_pg_pool_admin';
const ADMIN_NAME = 'E2E-PG-Pool-Admin';

function errorCodeOf(error: unknown): string {
  const message = error instanceof Error ? error.message : String(error);
  return message.split(':', 1)[0] ?? 'unknown';
}

describe('[tester] postgres pool key and rotation journey (CM-67 D)', function () {
  this.timeout(240000);

  const stamp = Date.now().toString(36);
  const dbA = `e2e_pool_${stamp}_a`;
  const dbB = `e2e_pool_${stamp}_b`;
  let admin: string | undefined;
  let profileRevision = '';
  const facts = { backendsA: -1, backendsB: -1, afterRotation: '', release: '' };
  let stage = 'setup';

  async function adminSql(sql: string): Promise<unknown[][]> {
    if (!admin) throw new Error('admin session unavailable');
    const payload = await invokeBackend<{
      rows?: unknown[][];
      results?: Array<{ rows?: unknown[][] }>;
    }>('execute_query', { dbSessionId: admin, sql });
    return payload.rows ?? payload.results?.[0]?.rows ?? [];
  }

  /** Backends on `database` that belong to this journey (the admin does not count itself). */
  async function backendCount(database: string): Promise<number> {
    const rows = await adminSql(
      `SELECT count(*)::int AS c FROM pg_stat_activity ` +
        `WHERE datname = '${database}' AND pid <> pg_backend_pid()`,
    );
    return Number(rows[0]?.[0] ?? -1);
  }

  async function openSession(database: string, ownerKey: string): Promise<SessionView> {
    const identity = await invokeBackend<{ clientInstanceId: string }>('get_platform_identity');
    const issued = await invokeBackend<{ idempotencyKey: string }>('issue_submission_token', {
      request: { operation: 'openSession' },
    });
    const opened = await invokeBackend<{ session: SessionView }>('open_session', {
      request: {
        initialTarget: {
          connectionId: ADMIN_ID,
          namespace: { database, catalog: null, schema: 'public', path: [] },
          object: null,
        },
        owner: {
          kind: 'editor',
          clientInstanceId: identity.clientInstanceId,
          editorSessionId: ownerKey,
        },
        idempotencyKey: issued.idempotencyKey,
      },
    });
    return opened.session;
  }

  /** Execute `sql` on `handle` and return the first scalar cell of the artifact rows. */
  async function scalar(handle: Handle, sql: string): Promise<string> {
    const session = await invokeBackend<SessionView>('get_session', { request: { handle } });
    const issued = await invokeBackend<{ idempotencyKey: string }>('issue_submission_token', {
      request: { operation: 'executeInSession', handle },
    });
    const receipt = await invokeBackend<{ executionId: string }>('execute_in_session', {
      request: {
        handle,
        expectedContextRevision: session.contextRevision,
        call: { command: 'query', input: { sql } },
        idempotencyKey: issued.idempotencyKey,
      },
    });
    let execution: Execution = { state: 'accepted', artifactIds: [], errorCode: null };
    await browser.waitUntil(
      async () => {
        execution = await invokeBackend<Execution>('get_execution', {
          request: { executionId: receipt.executionId },
        });
        return ['succeeded', 'failed', 'cancelled'].includes(execution.state);
      },
      { timeout: 30000, interval: 50, timeoutMsg: 'platform execution did not terminate' },
    );
    if (execution.state !== 'succeeded')
      throw new Error(`execution ${execution.state}: ${execution.errorCode ?? 'unknown'}`);
    const rows: unknown[][] = [];
    for (const artifactId of execution.artifactIds) {
      let index = 0;
      for (;;) {
        const chunk = await invokeBackend<{ bytes: number[]; publishedChunkCount: string }>(
          'read_artifact_chunk',
          { request: { artifactId, chunkIndex: String(index) } },
        );
        const payload = JSON.parse(new TextDecoder().decode(new Uint8Array(chunk.bytes))) as {
          output?: { rows?: unknown[][] };
          rows?: unknown[][];
        };
        rows.push(...(payload.output?.rows ?? payload.rows ?? []));
        if (++index >= Number(chunk.publishedChunkCount)) break;
      }
    }
    return String(rows[0]?.[0] ?? '');
  }

  async function closeSession(handle: Handle): Promise<{ resourceRelease: string }> {
    return invokeBackend<{ resourceRelease: string }>('close_session', {
      request: { handle, mode: 'rollbackAndClose' },
    });
  }

  before(async () => {
    await invokeBackend('save_connection', {
      config: {
        id: ADMIN_ID,
        name: ADMIN_NAME,
        databaseType: 'postgresql',
        host: process.env.E2E_PG_HOST || '127.0.0.1',
        port: Number(process.env.E2E_PG_PORT) || 5432,
        username: process.env.E2E_PG_USER || 'postgres',
        password: process.env.E2E_PG_PASSWORD || '',
        database: process.env.E2E_PG_DB || 'postgres',
        group: 'E2E 测试',
        colorTag: 'purple',
        sslMode: 'disable',
        options: {},
      },
    });
    admin = await invokeBackend<string>('connect', { connectionId: ADMIN_ID });
    await withSafeModeOff(async () => {
      await adminSql(`CREATE DATABASE ${dbA}`);
      await adminSql(`CREATE DATABASE ${dbB}`);
    });
    const profiles = await invokeBackend<ProfileView[]>('platform_list_profiles', { request: {} });
    const profile = profiles.find((entry) => entry.connectionId === ADMIN_ID);
    if (!profile) throw new Error('disposable profile unavailable');
    profileRevision = profile.configRevision;
  });

  after(async () => {
    try {
      await withSafeModeOff(async () => {
        await adminSql(
          `SELECT pg_terminate_backend(pid) FROM pg_stat_activity ` +
            `WHERE datname IN ('${dbA}', '${dbB}') AND pid <> pg_backend_pid()`,
        );
        await adminSql(`DROP DATABASE IF EXISTS ${dbA}`);
        await adminSql(`DROP DATABASE IF EXISTS ${dbB}`);
      });
    } catch {
      /* best-effort fixture cleanup */
    }
    if (admin) await invokeBackend('disconnect', { dbSessionId: admin }).catch(() => undefined);
    await invokeBackend('delete_connection', { id: ADMIN_ID }).catch(() => undefined);
  });

  it('keys resources by target and refuses the retired generation after rotation', async () => {
    let sessionA: SessionView | undefined;
    let sessionB: SessionView | undefined;
    let sessionA2: SessionView | undefined;
    let sessionNew: SessionView | undefined;
    try {
      stage = 'one-session-per-target';
      sessionA = await openSession(dbA, `pool-a-${stamp}`);
      expect(await scalar(sessionA.handle, 'SELECT current_database() AS db')).toBe(dbA);
      facts.backendsA = await backendCount(dbA);
      expect(facts.backendsA).toBe(1);

      sessionB = await openSession(dbB, `pool-b-${stamp}`);
      expect(await scalar(sessionB.handle, 'SELECT current_database() AS db')).toBe(dbB);
      facts.backendsB = await backendCount(dbB);
      expect(facts.backendsB).toBe(1);

      // Interleaved reads: two targets, two sessions, no cross-talk (CM-08 D side).
      stage = 'interleaved-reads';
      for (let round = 0; round < 5; round += 1) {
        expect(await scalar(sessionA.handle, 'SELECT current_database() AS db')).toBe(dbA);
        expect(await scalar(sessionB.handle, 'SELECT current_database() AS db')).toBe(dbB);
      }

      // A second session on the same database is a *separate* physical resource,
      // and closing it returns the count exactly (no leak, no over-count).
      stage = 'second-session-accounting';
      sessionA2 = await openSession(dbA, `pool-a2-${stamp}`);
      expect(await backendCount(dbA)).toBe(facts.backendsA + 1);
      const closedA2 = await closeSession(sessionA2.handle);
      facts.release = closedA2.resourceRelease;
      sessionA2 = undefined;
      expect(await backendCount(dbA)).toBe(facts.backendsA);

      // Rotation: a benign CAS patch still bumps the config revision, which is
      // exactly what retires the previous generation.
      stage = 'rotation';
      const rotation = await invokeBackend<ProfileView>('platform_update_profile', {
        request: {
          connectionId: ADMIN_ID,
          expectedConfigRevision: profileRevision,
          patch: { name: `${ADMIN_NAME} rotated` },
        },
      });
      expect(String(rotation.configRevision)).not.toBe(profileRevision);

      // The retired generation is refused for reads *and* executions…
      stage = 'retired-generation-refused';
      let getError = '';
      try {
        await invokeBackend('get_session', { request: { handle: sessionA.handle } });
      } catch (error) {
        getError = errorCodeOf(error);
      }
      facts.afterRotation = getError;
      expect(getError).toBe('configRevisionMismatch');

      let executeError = '';
      try {
        await scalar(sessionA.handle, 'SELECT 1 AS retired_probe');
      } catch (error) {
        executeError = errorCodeOf(error);
      }
      expect(executeError).toBe('configRevisionMismatch');

      // …while a session opened after the rotation works on the new generation.
      stage = 'new-generation-session';
      sessionNew = await openSession(dbA, `pool-new-${stamp}`);
      expect(await scalar(sessionNew.handle, 'SELECT current_database() AS db')).toBe(dbA);
      profileRevision = String(rotation.configRevision);

      // Every session can still be closed, and the backends really go away.
      stage = 'release-everything';
      const closedA = await closeSession(sessionA.handle);
      expect(closedA.resourceRelease).toBe('confirmed');
      await closeSession(sessionB.handle);
      await closeSession(sessionNew.handle);
      sessionA = undefined;
      sessionB = undefined;
      sessionNew = undefined;
      expect(await backendCount(dbA)).toBe(0);
      expect(await backendCount(dbB)).toBe(0);
    } catch (error) {
      const category =
        error instanceof Error &&
        ['WebDriverError', 'TimeoutError', 'AssertionError'].includes(error.name)
          ? error.name
          : 'Error';
      throw new Error(
        `CM-67 D journey failed at ${stage} (${category}); backendsA=${facts.backendsA}; ` +
          `backendsB=${facts.backendsB}; rotationCode=${facts.afterRotation}; release=${facts.release}`,
      );
    } finally {
      for (const session of [sessionA2, sessionNew, sessionB, sessionA]) {
        if (session) await closeSession(session.handle).catch(() => undefined);
      }
    }
  });
});
