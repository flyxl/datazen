import { describe, expect, it } from 'vitest';

import { createBackendClient, toId, toTimestamp, type SubmissionToken } from '../src/index';
import { createFakeTransport } from './client.test';

/** Execution domain wire contracts, migrated from the pre-F3 facade suite. */

const execId = (value: string) => {
  const narrowed = toId(value);
  if (narrowed === undefined) throw new Error(`fixture ${value} did not narrow`);
  return narrowed;
};

const TOKEN: SubmissionToken = {
  idempotencyKey: 'idem-1',
  expiresAt: (() => {
    const narrowed = toTimestamp(1_700_000_000_000);
    if (narrowed === undefined) throw new Error('fixture timestamp did not narrow');
    return narrowed;
  })(),
};

describe('Execution domain client', () => {
  it('routes cancelExecution under its own method name and passes the id through', async () => {
    const fake = createFakeTransport();
    fake.responses.set('cancelExecution', {
      executionId: execId('exec-7'),
      disposition: 'requested',
      state: 'cancelRequested',
    });

    const client = createBackendClient('desktop', fake.transport);
    await client.cancelExecution(execId('exec-7'), TOKEN);

    expect(fake.calls[0]).toEqual({
      method: 'cancelExecution',
      payload: { executionId: execId('exec-7'), submissionToken: TOKEN },
    });
  });

  for (const [disposition, state] of [
    ['requested', 'cancelRequested'],
    ['unsupported', 'running'],
    ['alreadyFinished', 'succeeded'],
  ]) {
    it(`CM-72 preserves ${disposition} as control disposition without inventing an execution effect`, async () => {
      const fake = createFakeTransport();
      fake.responses.set('cancelExecution', { executionId: execId('exec-7'), disposition, state });
      const client = createBackendClient('desktop', fake.transport);
      await expect(client.cancelExecution(execId('exec-7'), TOKEN)).resolves.toEqual({
        executionId: execId('exec-7'),
        disposition,
        state,
      });
    });
  }

  for (const payload of [
    { executionId: 'exec-7', accepted: true, state: 'cancelRequested' },
    { executionId: 'exec-7', disposition: 'Requested', state: 'cancelRequested' },
    { executionId: 'exec-7', disposition: 'requested', state: 'rolledBack' },
    { executionId: 'other-execution', disposition: 'requested', state: 'cancelRequested' },
  ]) {
    it(`CM-72 rejects a drifted or mismatched cancellation receipt ${JSON.stringify(payload)}`, async () => {
      const fake = createFakeTransport();
      fake.responses.set('cancelExecution', payload);
      const client = createBackendClient('desktop', fake.transport);
      await expect(client.cancelExecution(execId('exec-7'), TOKEN)).rejects.toMatchObject({
        code: 'ServiceUnavailable',
      });
      expect(fake.calls).toHaveLength(1);
    });
  }
});
