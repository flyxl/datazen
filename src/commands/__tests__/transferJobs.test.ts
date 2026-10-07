/**
 * IPC contract for the P5 Job command layer.
 *
 * These assertions pin the *host* names and the camelCase wire shape: they are
 * the boundary the backend owns, so a silent rename here would only surface as
 * an opaque rejection inside a live E2E run. The legacy `execute_data_transfer`
 * assertion is the cutover guard — the window must never reach it again.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { TransferJob } from '../transfer';
import { LOCAL_BACKEND_SCOPE, localBackendScope } from '../transferJobs';

const invokeMock = vi.fn();
type CapturedJobCall = { command: string; args: unknown; error?: string };
const e2eGlobal = globalThis as typeof globalThis & {
  __dataTransferJobCalls?: CapturedJobCall[];
};

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

const job: TransferJob = {
  source: { dbSessionId: 'src-session', database: 'appdb' },
  target: { dbSessionId: 'tgt-session', database: 'appdb_t' },
  mode: 'data',
  writeMode: 'insert',
  tables: [{ sourceTable: 'users', targetTable: 'users_v2', enabled: true }],
  options: { batchSize: 500 },
};

function invokedCommands(): string[] {
  return invokeMock.mock.calls.map((call) => String(call[0]));
}

describe('transferJobCommands', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.stubEnv('VITE_E2E', '');
    delete e2eGlobal.__dataTransferJobCalls;
    invokeMock.mockResolvedValue({ jobId: 'job-1', planId: 'plan-1' });
  });

  afterEach(() => {
    vi.unstubAllEnvs();
    delete e2eGlobal.__dataTransferJobCalls;
  });

  it('prepares through prepare_data_transfer_job with the declared §8 scope', async () => {
    const { transferJobCommands } = await import('../transferJobs');
    const request = { job, backendScope: localBackendScope(), idempotencyKey: 'k-prepare-1' };

    await transferJobCommands.prepare(request);

    expect(invokeMock).toHaveBeenCalledWith('prepare_data_transfer_job', { request });
    expect(invokedCommands()).not.toContain('apply_data_transfer_job');
  });

  it('spends the plan through apply_data_transfer_job with exactly the host fields', async () => {
    const { transferJobCommands } = await import('../transferJobs');
    const request = {
      planId: 'plan-1',
      planDigest: 'digest-1',
      selectionRevision: 3,
      selection: { sourceTables: ['users'] },
      confirmedDestructive: false,
      backendScope: localBackendScope(),
      idempotencyKey: 'data-transfer/apply/plan-1',
    };

    await transferJobCommands.apply(request);

    expect(invokeMock).toHaveBeenCalledWith('apply_data_transfer_job', { request });
    // The Rust request is `deny_unknown_fields`, so the camelCase key set is
    // the contract — an extra or missing field is a refusal, not a warning.
    expect(Object.keys(request).sort()).toEqual([
      'backendScope',
      'confirmedDestructive',
      'idempotencyKey',
      'planDigest',
      'planId',
      'selection',
      'selectionRevision',
    ]);
    expect(request.backendScope).toEqual({
      sourceBackendScope: LOCAL_BACKEND_SCOPE,
      targetBackendScope: LOCAL_BACKEND_SCOPE,
      profileBackendScopes: [],
    });
  });

  it('cancels through the legacy cancel_data_transfer command and keeps a false answer', async () => {
    invokeMock.mockResolvedValue(false);
    const { transferJobCommands } = await import('../transferJobs');

    await expect(transferJobCommands.cancel('job-1')).resolves.toBe(false);
    expect(invokeMock).toHaveBeenCalledWith('cancel_data_transfer', { jobId: 'job-1' });
  });

  it('never reaches the legacy execute_data_transfer command (§9 cutover)', async () => {
    const { transferJobCommands } = await import('../transferJobs');

    await transferJobCommands.prepare({ job, backendScope: localBackendScope() });
    await transferJobCommands.apply({
      planId: 'plan-1',
      planDigest: 'digest-1',
      selectionRevision: 1,
      selection: {},
      confirmedDestructive: true,
      backendScope: localBackendScope(),
    });
    await transferJobCommands.cancel('job-1');

    expect(invokedCommands()).toEqual([
      'prepare_data_transfer_job',
      'apply_data_transfer_job',
      'cancel_data_transfer',
    ]);
  });

  it('records every Job call for WDIO when VITE_E2E is on', async () => {
    vi.stubEnv('VITE_E2E', '1');
    const captures: CapturedJobCall[] = [];
    e2eGlobal.__dataTransferJobCalls = captures;
    const { transferJobCommands } = await import('../transferJobs');

    await transferJobCommands.cancel('job-7');
    invokeMock.mockRejectedValueOnce(new Error('refused'));
    await expect(transferJobCommands.cancel('job-8')).rejects.toThrow('refused');

    expect(captures).toHaveLength(2);
    expect(captures[0]?.command).toBe('cancel_data_transfer');
    expect(captures[0]?.args).toEqual({ jobId: 'job-7' });
    expect(captures[1]?.error).toContain('refused');
  });
});