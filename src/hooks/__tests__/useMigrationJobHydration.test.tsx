import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, renderHook, waitFor } from '@testing-library/react';
import {
  clearBackendClients,
  setBackendClient,
  type BackendClient,
  type JobView,
} from '@datazen/backend-client';

import { useMigrationJobHydration } from '../useMigrationJobHydration';

function jobView(overrides: Partial<JobView> = {}): JobView {
  return {
    jobId: 'transfer-apply-1' as JobView['jobId'],
    kind: 'dataTransferApply',
    state: 'running',
    stage: 'data',
    executionIds: [],
    artifactIds: [],
    createdAt: 1 as JobView['createdAt'],
    updatedAt: 1 as JobView['updatedAt'],
    effectOutcome: null,
    cancelRequested: false,
    pendingVerificationReason: null,
    progress: {
      read: 1 as JobView['progress']['read'],
      converted: 1 as JobView['progress']['converted'],
      attempted: 1 as JobView['progress']['attempted'],
      committed: 0 as JobView['progress']['committed'],
      unknown: 0 as JobView['progress']['unknown'],
    },
    ...overrides,
  };
}

describe('useMigrationJobHydration', () => {
  afterEach(() => {
    cleanup();
    clearBackendClients();
  });

  it('refreshes the active and terminal projection from watch updates and only stops on unmount', async () => {
    let onUpdate: ((view: JobView, meta: { resubscribed: boolean }) => void) | undefined;
    const stop = vi.fn();
    const cancelJob = vi.fn();
    const active = jobView();
    const terminal = jobView({
      state: 'cancelled',
      cancelRequested: true,
      effectOutcome: 'rolledBack',
      updatedAt: 2 as JobView['updatedAt'],
    });
    const client = {
      backendId: 'test',
      listJobs: vi.fn(async (filter: Record<string, unknown>) =>
        filter['states'] ? [active] : [active],
      ),
      getJob: vi.fn(async () => active),
      watchJob: vi.fn((_jobId, callback) => {
        onUpdate = callback;
        return { stop };
      }),
      cancelJob,
    } as unknown as BackendClient;
    setBackendClient('test', client);

    const hook = renderHook(() => useMigrationJobHydration('dataTransfer'));
    await waitFor(() => expect(hook.result.current.hydration?.activeJobs).toHaveLength(1));
    expect(hook.result.current.hydration?.jobs[0]?.progress.read).toBe(1);
    expect(client.watchJob).toHaveBeenCalledWith(
      'transfer-apply-1',
      expect.any(Function),
      expect.objectContaining({ intervalMs: 2000 }),
    );

    act(() => onUpdate?.(terminal, { resubscribed: false }));

    expect(hook.result.current.hydration?.activeJobs).toEqual([]);
    expect(hook.result.current.hydration?.terminalJobs).toEqual([terminal]);
    expect(hook.result.current.hydration?.jobs).toEqual([terminal]);
    expect(cancelJob).not.toHaveBeenCalled();

    hook.unmount();
    expect(stop).toHaveBeenCalledTimes(1);
    expect(cancelJob).not.toHaveBeenCalled();
  });
});
