import { describe, expect, it, vi, beforeEach } from 'vitest';

const { invokeMock, listenMock } = vi.hoisted(() => ({
  invokeMock: vi.fn(),
  listenMock: vi.fn(async () => () => {}),
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: (...args: unknown[]) => listenMock(...args),
}));

import { runSqlFileExecution } from '../sqlFileExecution';

const t = ((key: string) => key) as unknown as (
  key: never,
  params?: Record<string, string | number>,
) => string;

function baseOptions() {
  return {
    dbSessionId: 'live-1',
    database: 'app',
    t,
  };
}

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) =>
    cmd === 'get_tables' ? [] : true,
  );
});

describe('runSqlFileExecution (decision 3+6 unified IPC)', () => {
  it('invokes the merged restore_sql_file command with session + database + options', async () => {
    const ok = await runSqlFileExecution(baseOptions());
    expect(ok).toBe(true);
    expect(invokeMock).toHaveBeenCalledWith('restore_sql_file', {
      dbSessionId: 'live-1',
      database: 'app',
      options: [],
    });
    // The four former entry points must no longer be reachable from the wrapper.
    for (const gone of [
      'restore_database',
      'restore_database_with_dialog',
      'execute_sql_file',
      'execute_sql_file_with_dialog',
    ]) {
      expect(invokeMock).not.toHaveBeenCalledWith(gone, expect.anything());
    }
    expect(listenMock).toHaveBeenCalledWith('restore-progress', expect.any(Function));
  });

  it('pushes the overwrite option after user confirmation on a non-empty database', async () => {
    invokeMock.mockImplementation(async (cmd: string) =>
      cmd === 'get_tables' ? [{ name: 'users' }] : true,
    );
    const confirmOverwrite = vi.fn().mockResolvedValue(true);

    await runSqlFileExecution({ ...baseOptions(), confirmOverwrite });

    expect(confirmOverwrite).toHaveBeenCalledWith(1);
    expect(invokeMock).toHaveBeenCalledWith('restore_sql_file', {
      dbSessionId: 'live-1',
      database: 'app',
      options: ['overwrite'],
    });
  });

  it('aborts before restore when overwrite is declined on a non-empty database', async () => {
    invokeMock.mockImplementation(async (cmd: string) =>
      cmd === 'get_tables' ? [{ name: 'users' }] : true,
    );
    const ok = await runSqlFileExecution({
      ...baseOptions(),
      confirmOverwrite: vi.fn().mockResolvedValue(false),
    });
    expect(ok).toBe(false);
    // get_tables probing is allowed; the restore itself must never start.
    expect(invokeMock).not.toHaveBeenCalledWith('restore_sql_file', expect.anything());
  });

  it('aborts before any IPC when the pre-execute confirmation is declined', async () => {
    const ok = await runSqlFileExecution({
      ...baseOptions(),
      confirmBeforeExecute: vi.fn().mockResolvedValue(false),
    });
    expect(ok).toBe(false);
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it('returns false when the dialog is dismissed (backend reports not executed)', async () => {
    const ok = await runSqlFileExecution(baseOptions());
    expect(ok).toBe(true);
    invokeMock.mockImplementation(async (cmd: string) =>
      cmd === 'get_tables' ? [] : false,
    );
    const cancelled = await runSqlFileExecution(baseOptions());
    expect(cancelled).toBe(false);
  });

  it('surfaces backend errors through onError and rethrows', async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === 'get_tables') return [];
      throw new Error('boom');
    });
    const onError = vi.fn();
    await expect(runSqlFileExecution({ ...baseOptions(), onError })).rejects.toThrow('boom');
    expect(onError).toHaveBeenCalledWith('boom');
  });
});
