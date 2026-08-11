import { expect, browser } from '@wdio/globals';

async function invokeBackend<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  const result = await browser.executeAsync(
    (c: string, a: string, done: (r: unknown) => void) => {
      (window as unknown as { __TAURI_INTERNALS__?: { invoke: (cmd: string, args: unknown) => Promise<unknown> } })
        .__TAURI_INTERNALS__
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
    const definitions = await invokeBackend<{ id: string }[]>('get_connection_commands', {
      connectionId,
    });
    expect(definitions.some((d) => d.id === 'query')).toBe(true);

    const result = await invokeBackend<{ data: unknown }>('execute_driver_command', {
      request: {
        connectionId,
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
    await expect(
      invokeBackend('execute_driver_command', {
        request: {
          connectionId: conns[0].id,
          command: 'not-a-real-command',
          input: {},
        },
      }),
    ).rejects.toThrow(/Unsupported driver command/);
  });
});
