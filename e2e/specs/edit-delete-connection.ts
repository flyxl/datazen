import { expect, browser } from '@wdio/globals';

async function invokeBackend<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  const result = await browser.executeAsync(
    (c: string, a: string, done: (r: any) => void) => {
      (window as any).__TAURI_INTERNALS__
        .invoke(c, JSON.parse(a))
        .then((r: any) => done(r))
        .catch((e: any) => done({ __error: String(e) }));
    },
    cmd,
    JSON.stringify(args),
  );
  if (result && typeof result === 'object' && '__error' in (result as any)) {
    throw new Error((result as any).__error);
  }
  return result as T;
}

interface Conn {
  id: string;
  name: string;
  host: string;
  port: number;
  database: string;
  username: string;
  password: string;
  databaseType: string;
  group?: string;
  color?: string;
  sslMode?: string;
  lastConnectedAt?: string | null;
}

const TEST_CONN: Conn = {
  id: 'e2e-edit-delete-test',
  name: 'E2E-编辑删除测试',
  host: 'localhost',
  port: 5432,
  database: 'postgres',
  username: 'postgres',
  password: '',
  databaseType: 'postgresql',
  group: '',
  color: '',
  sslMode: 'prefer',
  lastConnectedAt: null,
};

describe('编辑、复制和删除连接 (CM-003, CM-004, CM-006)', () => {
  before(async () => {
    await invokeBackend('save_connection', { config: TEST_CONN });
    await browser.pause(500);
  });

  after(async () => {
    // Clean up: delete the test connection and any copies
    const conns = await invokeBackend<Conn[]>('get_connections');
    for (const c of conns) {
      if (c.id === TEST_CONN.id || c.name.startsWith('E2E-编辑删除') || c.name.startsWith('E2E-已编辑')) {
        await invokeBackend('delete_connection', { id: c.id });
      }
    }
    await browser.pause(300);
  });

  // ── Save & Read (CM-003) ──

  it('CM-003a: save_connection should persist a new connection', async () => {
    const conns = await invokeBackend<Conn[]>('get_connections');
    const found = conns.find((c) => c.id === TEST_CONN.id);
    expect(found).toBeDefined();
    expect(found!.name).toBe(TEST_CONN.name);
    expect(found!.host).toBe('localhost');
  });

  // ── Edit (CM-003) ──

  it('CM-003b: editing a connection name should persist', async () => {
    const updated = { ...TEST_CONN, name: 'E2E-已编辑连接' };
    await invokeBackend('save_connection', { config: updated });

    const conns = await invokeBackend<Conn[]>('get_connections');
    const found = conns.find((c) => c.id === TEST_CONN.id);
    expect(found).toBeDefined();
    expect(found!.name).toBe('E2E-已编辑连接');

    // Restore original name
    await invokeBackend('save_connection', { config: TEST_CONN });
  });

  it('CM-003c: editing connection host/port should persist', async () => {
    const updated = { ...TEST_CONN, host: '192.168.1.100', port: 5433 };
    await invokeBackend('save_connection', { config: updated });

    const conns = await invokeBackend<Conn[]>('get_connections');
    const found = conns.find((c) => c.id === TEST_CONN.id);
    expect(found!.host).toBe('192.168.1.100');
    expect(found!.port).toBe(5433);

    // Restore
    await invokeBackend('save_connection', { config: TEST_CONN });
  });

  // ── Duplicate (CM-006) ──

  it('CM-006: duplicating a connection creates a copy with different id', async () => {
    const connsBefore = await invokeBackend<Conn[]>('get_connections');
    const countBefore = connsBefore.filter((c) => c.name.includes('E2E-编辑删除')).length;

    // Simulate duplicate: create a copy with a new id
    const copy: Conn = {
      ...TEST_CONN,
      id: `${TEST_CONN.id}-copy-${Date.now()}`,
      name: `${TEST_CONN.name} (副本)`,
    };
    await invokeBackend('save_connection', { config: copy });

    const connsAfter = await invokeBackend<Conn[]>('get_connections');
    const countAfter = connsAfter.filter((c) => c.name.includes('E2E-编辑删除')).length;
    expect(countAfter).toBe(countBefore + 1);

    const foundCopy = connsAfter.find((c) => c.id === copy.id);
    expect(foundCopy).toBeDefined();
    expect(foundCopy!.name).toContain('副本');

    // Clean up copy
    await invokeBackend('delete_connection', { id: copy.id });
  });

  // ── Delete (CM-004) ──

  it('CM-004: deleting a connection removes it from the list', async () => {
    // Create a disposable connection
    const disposable: Conn = {
      ...TEST_CONN,
      id: `e2e-disposable-${Date.now()}`,
      name: 'E2E-编辑删除-临时',
    };
    await invokeBackend('save_connection', { config: disposable });

    let conns = await invokeBackend<Conn[]>('get_connections');
    expect(conns.find((c) => c.id === disposable.id)).toBeDefined();

    await invokeBackend('delete_connection', { id: disposable.id });

    conns = await invokeBackend<Conn[]>('get_connections');
    expect(conns.find((c) => c.id === disposable.id)).toBeUndefined();
  });

  it('CM-004b: delete should be idempotent (no error on double delete)', async () => {
    const disposable: Conn = {
      ...TEST_CONN,
      id: `e2e-disposable2-${Date.now()}`,
      name: 'E2E-编辑删除-临时2',
    };
    await invokeBackend('save_connection', { config: disposable });
    await invokeBackend('delete_connection', { id: disposable.id });

    // Second delete should not throw
    let error: string | null = null;
    try {
      await invokeBackend('delete_connection', { id: disposable.id });
    } catch (e) {
      error = String(e);
    }
    expect(error).toBeNull();
  });
});
