import { describe, expect, it, vi, beforeEach } from 'vitest';

const { invokeMock, listenMock } = vi.hoisted(() => ({
  invokeMock: vi.fn(),
  listenMock: vi.fn(async (..._args: unknown[]) => () => {}),
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: (...args: unknown[]) => listenMock(...args),
}));

import { runSqlFileExecution } from '../sqlFileExecution';
import type { I18nKey } from '../../locales';

/** Identity translator: the key is what the progress line would carry anyway. */
const t = (key: I18nKey): string => key;

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
    cmd === 'execute_driver_command'
      ? { data: { database: 'app', schemas: [], relations: [] } }
      : true,
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
      cmd === 'execute_driver_command'
        ? {
            data: {
              database: 'app',
              schemas: [],
              relations: [
                {
                  ref: { database: 'app', schema: null, name: 'users' },
                  kind: 'table',
                  rowCount: null,
                },
              ],
            },
          }
        : true,
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
      cmd === 'execute_driver_command'
        ? {
            data: {
              database: 'app',
              schemas: [],
              relations: [
                {
                  ref: { database: 'app', schema: null, name: 'users' },
                  kind: 'table',
                  rowCount: null,
                },
              ],
            },
          }
        : true,
    );
    const ok = await runSqlFileExecution({
      ...baseOptions(),
      confirmOverwrite: vi.fn().mockResolvedValue(false),
    });
    expect(ok).toBe(false);
    // Catalog probing is allowed; the restore itself must never start.
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
      cmd === 'execute_driver_command'
        ? { data: { database: 'app', schemas: [], relations: [] } }
        : false,
    );
    const cancelled = await runSqlFileExecution(baseOptions());
    expect(cancelled).toBe(false);
  });

  it('surfaces backend errors through onError and rethrows', async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === 'execute_driver_command')
        return { data: { database: 'app', schemas: [], relations: [] } };
      throw new Error('boom');
    });
    const onError = vi.fn();
    await expect(runSqlFileExecution({ ...baseOptions(), onError })).rejects.toThrow('boom');
    expect(onError).toHaveBeenCalledWith('boom');
  });
  it('does not report success or retry when the restore Job is unsupported', async () => {
    const failure = new Error('Restore Unsupported: no verified Gateway binding');
    invokeMock.mockRejectedValue(failure);
    const onError = vi.fn();
    const progress = vi.fn();
    await expect(runSqlFileExecution({ ...baseOptions(), onError, onProgress: progress })).rejects.toThrow('Restore Unsupported');
    expect(onError).toHaveBeenCalledWith(failure.message);
    expect(invokeMock.mock.calls.filter(([command]) => command === 'restore_sql_file')).toHaveLength(1);
    expect(progress.mock.calls.some(([payload]) => payload?.phase === 'done')).toBe(false);
  });

  it.each(['unknown', 'partiallyApplied', 'rolledBack', 'notStarted'])('preserves structured %s outcomes without retry or success', async (effectOutcome) => {
    const failure = { code: 'restoreOutcome', message: `Restore failed: ${effectOutcome}; automatic retry is forbidden`, automaticRetryAllowed: false, details: { job: { state: 'failed', effectOutcome } } };
    invokeMock.mockRejectedValue(failure);
    const onError = vi.fn();
    const progress = vi.fn();
    await expect(runSqlFileExecution({ ...baseOptions(), onError, onProgress: progress })).rejects.toEqual(failure);
    expect(onError).toHaveBeenCalledWith(failure.message);
    expect(invokeMock.mock.calls.filter(([command]) => command === 'restore_sql_file')).toHaveLength(1);
    expect(progress.mock.calls.some(([payload]) => payload?.phase === 'done')).toBe(false);
  });

});
