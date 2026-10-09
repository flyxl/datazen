import { emptyJobProgress } from '@datazen/backend-client';
import { renderHook } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { SchemaDiffDeployResult, SchemaDiffJobDetails } from '../../../commands/schemaDiff';
import type { SchemaDiffTrackedJob } from '../useSchemaDiffJobLifecycle';
import { useSchemaDiffJobRestore } from '../useSchemaDiffJobRestore';

const deployResult: SchemaDiffDeployResult = {
  status: 'committed',
  executedCount: 1,
  statementCount: 1,
  errors: [],
  statementResults: [],
};

function job(submittedLocally: boolean): SchemaDiffTrackedJob {
  // The hook only reads the domain result; Job DTO fields are exercised by
  // SchemaDiffJobStatusPanel and the real serialized JobDetails contract tests.
  const details = { deployResult } as unknown as SchemaDiffJobDetails;
  return {
    jobId: 'historical-apply',
    kind: 'schemaDiffApply',
    state: 'succeeded',
    progress: emptyJobProgress(),
    submittedLocally,
    details,
  };
}

describe('Schema Diff wizard job isolation', () => {
  it('keeps history separate, then adopts only the result submitted by this wizard', () => {
    const setDeployResult = vi.fn();
    const { rerender } = renderHook(
      ({ currentJob }) => useSchemaDiffJobRestore({ currentJob, setDeployResult }),
      { initialProps: { currentJob: job(false) } },
    );
    expect(setDeployResult).not.toHaveBeenCalled();
    rerender({ currentJob: job(true) });
    expect(setDeployResult).toHaveBeenCalledExactlyOnceWith(deployResult);
    setDeployResult.mockClear();
    rerender({ currentJob: job(false) });
    expect(setDeployResult).not.toHaveBeenCalled();
  });
});
