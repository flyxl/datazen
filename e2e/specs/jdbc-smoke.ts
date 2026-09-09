/**
 * Optional JDBC agent smoke.
 *
 * Requires:
 * - Build with `DATAZEN_DRIVERS=…,jdbc`
 * - `DATAZEN_E2E_JDBC=1`
 * - Agent jar on path / Settings
 * - Optional H2 (or other) connection already saved as type jdbc
 *
 * Without the env flag this suite no-ops so CI without JRE stays green.
 */
import { expect, browser } from '@wdio/globals';

async function invokeBackend<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  const result = await browser.executeAsync(
    (c: string, a: string, done: (r: unknown) => void) => {
      (
        window as unknown as {
          __TAURI_INTERNALS__?: { invoke: (cmd: string, args: unknown) => Promise<unknown> };
        }
      ).__TAURI_INTERNALS__
        ?.invoke(c, JSON.parse(a))
        .then((r) => done(r))
        .catch((e: unknown) => done({ __error: String(e) }));
    },
    cmd,
    JSON.stringify(args),
  );
  if (result && typeof result === 'object' && '__error' in (result as object)) {
    throw new Error(String((result as { __error: string }).__error));
  }
  return result as T;
}

describe('JDBC agent smoke', () => {
  it('connects and runs SELECT 1 when DATAZEN_E2E_JDBC=1', async () => {
    if (process.env.DATAZEN_E2E_JDBC !== '1') {
      return;
    }

    const conns = await invokeBackend<
      Array<{ id: string; databaseType?: string; database_type?: string }>
    >('get_connections');
    const jdbc = conns.find(
      (c) => (c.databaseType ?? c.database_type ?? '').toLowerCase() === 'jdbc',
    );
    if (!jdbc) {
      // Suite is opt-in; missing fixture is not a failure when flag is set for local explorers.
      console.warn('[jdbc-smoke] no jdbc connection in store; skip');
      return;
    }

    const dbSessionId = await invokeBackend<string>('connect', { connectionId: jdbc.id });
    try {
      const result = await invokeBackend<{ data: unknown }>('execute_driver_command', {
        request: {
          dbSessionId,
          command: 'query',
          input: { sql: 'SELECT 1 AS n' },
        },
      });
      expect(result.data).toBeDefined();
    } finally {
      await invokeBackend('disconnect', { dbSessionId }).catch(() => undefined);
    }
  });
});
