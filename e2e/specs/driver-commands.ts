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

describe('Driver Command IPC', () => {
  it('discovers commands from a connection and executes query', async () => {
    const conns = await invokeBackend<{ id: string }[]>('get_connections');
    if (conns.length === 0) {
      return;
    }
    const connectionId = conns[0].id;
    const dbSessionId = await invokeBackend<string>('connect', { connectionId });
    const definitions = await invokeBackend<{ id: string }[]>('get_connection_commands', {
      dbSessionId,
    });
    expect(definitions.some((d) => d.id === 'query')).toBe(true);

    // Runtime db session id → execute_driver_command contract slot
    // (do not rely on the backend resolve_session dual-mode fallback).
    const result = await invokeBackend<{ data: unknown }>('execute_driver_command', {
      request: {
        dbSessionId,
        command: 'query',
        input: { sql: 'SELECT 1 AS n' },
      },
    });
    expect(result.data).toBeDefined();
  });

  it('rejects an unsupported driver command', async () => {
    const conns = await invokeBackend<{ id: string }[]>('get_connections');
    if (conns.length === 0) {
      return;
    }
    const dbSessionId = await invokeBackend<string>('connect', {
      connectionId: conns[0].id,
    });
    await expect(
      invokeBackend('execute_driver_command', {
        request: {
          dbSessionId,
          command: 'not-a-real-command',
          input: {},
        },
      }),
    ).rejects.toThrow(/Unsupported driver command/);
  });
});
