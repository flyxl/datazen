import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { TransferProfile } from '../transfer';

const invokeMock = vi.fn();
type CapturedTransferRun = {
  args: { request: { planId: string }; profile?: { id: string; revision: string } };
  response?: { tables: unknown[]; rowsInserted: number };
  error?: string;
};
const e2eGlobal = globalThis as typeof globalThis & {
  __dataTransferRunCalls?: CapturedTransferRun[];
};

import { setBackendClient } from '@datazen/backend-client';
// form-split R6: the invoke wrappers ride the bound backend transport now
// (same binding pattern as file.test.ts — the mock keeps its invoke shape).
setBackendClient('test', {
  transport: { call: (c: string, p?: unknown) => (p === undefined ? invokeMock(c) : invokeMock(c, p)) },
} as never);
describe('transferCommands.inspect', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.stubEnv('VITE_E2E', '');
    delete e2eGlobal.__dataTransferRunCalls;
    invokeMock.mockResolvedValue([]);
  });

  afterEach(() => {
    vi.unstubAllEnvs();
    delete e2eGlobal.__dataTransferRunCalls;
  });

  it('passes source-only SQL-file inspection to the host', async () => {
    const { transferCommands } = await import('../transfer');

    await transferCommands.inspectSqlFile(
      'src-session-uuid',
      'structureAndData',
      'goecoride',
      'public',
      'mysql',
      [
        {
          sourceTable: 'users',
          targetTable: 'users_copy',
          createNew: true,
          enabled: true,
          columnMappings: [{ sourceColumn: 'id', targetColumn: 'user_id', skip: false }],
        },
      ],
    );

    expect(invokeMock).toHaveBeenCalledWith('inspect_sql_file_transfer', {
      sourceDbSessionId: 'src-session-uuid',
      sourceDatabase: 'goecoride',
      sourceSchema: 'public',
      targetDatabaseType: 'mysql',
      mode: 'structureAndData',
      tables: [
        {
          sourceTable: 'users',
          targetTable: 'users_copy',
          createNew: true,
          enabled: true,
          columnMappings: [{ sourceColumn: 'id', targetColumn: 'user_id', skip: false }],
        },
      ],
    });
  });

  it('passes db session ids to inspect_data_transfer IPC', async () => {
    const { transferCommands } = await import('../transfer');

    await transferCommands.inspect(
      'src-session-uuid',
      'tgt-session-uuid',
      'structureAndData',
      'goecoride',
      'datazen_test',
    );

    expect(invokeMock).toHaveBeenCalledWith('inspect_data_transfer', {
      sourceDbSessionId: 'src-session-uuid',
      targetDbSessionId: 'tgt-session-uuid',
      sourceDatabase: 'goecoride',
      targetDatabase: 'datazen_test',
      mode: 'structureAndData',
      tables: null,
    });
  });

  it('persists profiles through dedicated IPC commands', async () => {
    const { transferCommands } = await import('../transfer');
    const profile = {
      version: 1,
      id: 'profile-1',
      name: 'nightly',
      sourceConnectionId: 'src',
      destinationMode: 'sqlFile',
      mode: 'data',
      writeMode: 'insert',
      tables: [],
      options: { batchSize: 500, stopOnError: true, confirmedDestructive: false },
      createdAt: '2026-09-21T00:00:00.000Z',
      updatedAt: '2026-09-21T00:00:00.000Z',
    } satisfies TransferProfile;
    await transferCommands.getProfiles();
    await transferCommands.saveProfile(profile);
    await transferCommands.deleteProfile('profile-1');
    expect(invokeMock).toHaveBeenNthCalledWith(1, 'get_transfer_profiles');
    expect(invokeMock).toHaveBeenNthCalledWith(2, 'save_transfer_profile', { profile });
    expect(invokeMock).toHaveBeenNthCalledWith(3, 'delete_transfer_profile', {
      profileId: 'profile-1',
    });
  });

  it('executes only an opaque plan with controlled selection and options', async () => {
    const { transferCommands } = await import('../transfer');

    await transferCommands.execute({
      planId: 'opaque-plan-id',
      selection: { sourceTables: ['users'] },
      options: { confirmedDestructive: true },
      jobId: 'cancel-token',
    });

    expect(invokeMock).toHaveBeenCalledWith('execute_data_transfer', {
      request: {
        planId: 'opaque-plan-id',
        selection: { sourceTables: ['users'] },
        options: { confirmedDestructive: true },
        jobId: 'cancel-token',
      },
    });
  });

  it('records the real request and response only when the E2E recorder is enabled', async () => {
    vi.stubEnv('VITE_E2E', '1');
    const response = { tables: [], rowsInserted: 4 };
    invokeMock.mockResolvedValue(response);
    e2eGlobal.__dataTransferRunCalls = [];
    const { transferCommands } = await import('../transfer');
    const request = { planId: 'opaque-e2e-plan' };
    const profile = { id: 'profile-1', revision: 'revision-2' };

    await expect(transferCommands.execute(request, profile)).resolves.toBe(response);

    expect(invokeMock).toHaveBeenCalledWith('execute_data_transfer', { request, profile });
    expect(e2eGlobal.__dataTransferRunCalls).toEqual([{ args: { request, profile }, response }]);
  });

  it('records a rejected E2E request without swallowing its error', async () => {
    vi.stubEnv('VITE_E2E', '1');
    invokeMock.mockRejectedValue(new Error('commit outcome unknown'));
    e2eGlobal.__dataTransferRunCalls = [];
    const { transferCommands } = await import('../transfer');
    const request = { planId: 'opaque-e2e-plan' };

    await expect(transferCommands.execute(request)).rejects.toThrow('commit outcome unknown');

    expect(e2eGlobal.__dataTransferRunCalls).toEqual([
      { args: { request }, error: 'Error: commit outcome unknown' },
    ]);
  });

  it('routes pairing classification through the transfer IPC', async () => {
    const { transferCommands } = await import('../transfer');

    await transferCommands.classifyPair('postgres', 'mysql');

    expect(invokeMock).toHaveBeenCalledWith('classify_transfer_pair', {
      sourceDatabaseType: 'postgres',
      targetDatabaseType: 'mysql',
    });
  });

  it('forwards a complete job to preview and exposes the returned plan', async () => {
    const { transferCommands } = await import('../transfer');
    const job = {
      source: { dbSessionId: 'source-session', database: 'source-db' },
      target: { dbSessionId: 'target-session', database: 'target-db' },
      mode: 'data' as const,
      writeMode: 'insert' as const,
      tables: [],
      options: { batchSize: 10, stopOnError: true },
    };

    await transferCommands.preview(job);

    expect(invokeMock).toHaveBeenCalledWith('preview_data_transfer', { job });
  });

  it('routes cancellation by the opaque job token', async () => {
    const { transferCommands } = await import('../transfer');

    await transferCommands.cancel('cancel-token');

    expect(invokeMock).toHaveBeenCalledWith('cancel_data_transfer', { jobId: 'cancel-token' });
  });

  it('requests an opaque SQL-file destination from the native host dialog', async () => {
    const { transferCommands } = await import('../transfer');

    await transferCommands.pickSqlFile();

    expect(invokeMock).toHaveBeenCalledWith('pick_data_transfer_sql_file');
  });
});
