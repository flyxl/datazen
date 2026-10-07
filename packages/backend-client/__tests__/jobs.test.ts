import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import {
  clearBackendClients,
  createBackendClient,
  parseCancelReceipt,
  parseJobView,
  type BackendTransport,
  type Id,
  type MethodMap,
  type SubmissionToken,
  type Timestamp,
  toId,
  toTimestamp,
} from '../src/index';

const id = (value: string): Id => {
  const narrowed = toId(value);
  if (narrowed === undefined) throw new Error(`bad id ${value}`);
  return narrowed;
};

const timestamp = (value: number): Timestamp => {
  const narrowed = toTimestamp(value);
  if (narrowed === undefined) throw new Error(`bad timestamp`);
  return narrowed;
};

const TOKEN: SubmissionToken = {
  idempotencyKey: 'idem-apply-1',
  expiresAt: timestamp(1_700_000_000_000),
};

/**
 * The payload shape the Rust kernel actually puts on the wire.
 *
 * Counters are **decimal strings**, not numbers: `platform-api`'s `Counter` is a
 * `u64` whose `Serialize` uses `collect_str` (CM-01), because a JSON number would
 * lose precision above `2^53`. Instants are RFC-8601 strings. A fixture written
 * in the shape the client *wants* rather than the shape the client *gets* is
 * what let `toCounter` drift: every counter here was a JS number, so the suite
 * agreed with itself and with nothing else.
 *
 * The instants are built from the current clock rather than hard-coded, because
 * `submitJobIdempotent` only recovers a submission whose `createdAt` falls in a
 * recent window — a fixed date would silently fall outside it and turn those
 * recovery tests into "timed out" for a reason that has nothing to do with them.
 */
function rawJobView(overrides: Record<string, unknown> = {}): Record<string, unknown> {
  const now = new Date().toISOString();
  return {
    jobId: 'job-1',
    kind: 'schemaDiffApply',
    state: 'running',
    stage: 'apply',
    executionIds: ['exec-1'],
    artifactIds: ['art-1'],
    createdAt: now,
    updatedAt: now,
    effectOutcome: 'completed',
    cancelRequested: false,
    pendingVerificationReason: null,
    progress: { read: '3', converted: '2', attempted: '1', committed: '1', unknown: '0' },
    ...overrides,
  };
}

function createFakeTransport() {
  const calls: Array<{ method: keyof MethodMap; payload: unknown }> = [];
  const responses = new Map<keyof MethodMap, unknown>();
  const failures = new Map<keyof MethodMap, unknown>();

  const transport: BackendTransport = {
    async call<K extends keyof MethodMap>(method: K, payload: MethodMap[K]['request']) {
      calls.push({ method, payload });
      if (failures.has(method)) {
        const failure = failures.get(method);
        failures.delete(method);
        throw failure;
      }
      if (!responses.has(method)) throw new Error(`fake transport has no response for ${method}`);
      return responses.get(method) as MethodMap[K]['response'];
    },
  };

  return { transport, calls, responses, failures };
}

describe('parseJobView', () => {
  it('parses the full P5 DTO', () => {
    const job = parseJobView(rawJobView());
    expect(job.jobId).toBe('job-1');
    expect(job.state).toBe('running');
    expect(job.effectOutcome).toBe('completed');
    expect(job.cancelRequested).toBe(false);
    expect(job.pendingVerificationReason).toBeNull();
    expect(job.progress.committed).toBe(1);
    expect(job.executionIds).toEqual(['exec-1']);
  });

  // The regression this file exists for: the wire form is a decimal string, and
  // a client that only accepts numbers does not fail — it reads every counter as
  // 0, which looks exactly like a job that has done nothing.
  describe('counter wire form (CM-01)', () => {
    it('reads the counts the kernel actually sent', () => {
      const job = parseJobView(rawJobView());
      expect(job.progress).toEqual({
        read: 3,
        converted: 2,
        attempted: 1,
        committed: 1,
        unknown: 0,
      });
    });

    it('does not silently collapse a string counter to zero', () => {
      const job = parseJobView(
        rawJobView({
          progress: { read: '0', converted: '0', attempted: '0', committed: '0', unknown: '412' },
        }),
      );
      expect(job.progress.unknown).toBe(412);
    });

    it('still accepts a bare number for payloads with no u64 to encode', () => {
      const job = parseJobView(
        rawJobView({ progress: { read: 7, converted: 0, attempted: 0, committed: 0, unknown: 0 } }),
      );
      expect(job.progress.read).toBe(7);
    });

    it('reads a counter up to the largest value a JS number holds exactly', () => {
      const job = parseJobView(
        rawJobView({
          progress: {
            read: String(Number.MAX_SAFE_INTEGER),
            converted: 0,
            attempted: 0,
            committed: 0,
            unknown: 0,
          },
        }),
      );
      expect(job.progress.read).toBe(Number.MAX_SAFE_INTEGER);
    });

    it('refuses a present counter it cannot hold rather than reporting it as 0', () => {
      // Absent means "nothing has moved yet". A count of 2^53 + 1 does not mean
      // that, and reporting it as 0 is the exact lie this contract forbids.
      expect(() =>
        parseJobView(
          rawJobView({
            progress: {
              read: '9007199254740993',
              converted: '0',
              attempted: '0',
              committed: '0',
              unknown: '0',
            },
          }),
        ),
      ).toThrowError(/Malformed job progress/);
    });

    it('still defaults a genuinely absent counter to zero', () => {
      const job = parseJobView(
        rawJobView({ progress: { read: '5', converted: '0', attempted: '0' } }),
      );
      expect(job.progress).toEqual({
        read: 5,
        converted: 0,
        attempted: 0,
        committed: 0,
        unknown: 0,
      });
    });
  });

  it('defaults missing P5 fields instead of leaking undefined', () => {
    const job = parseJobView({
      jobId: 'job-2',
      kind: 'dataSyncApply',
      state: 'queued',
      stage: null,
      executionIds: [],
      artifactIds: [],
      createdAt: 1700000000000,
      updatedAt: 1700000000000,
    });
    expect(job.effectOutcome).toBeNull();
    expect(job.cancelRequested).toBe(false);
    expect(job.pendingVerificationReason).toBeNull();
    expect(job.progress).toEqual({ read: 0, converted: 0, attempted: 0, committed: 0, unknown: 0 });
  });

  it('normalizes an ISO timestamp', () => {
    const job = parseJobView(rawJobView({ createdAt: '2026-01-01T00:00:00Z' }));
    expect(job.createdAt).toBe(Date.parse('2026-01-01T00:00:00Z'));
  });

  it('rejects a malformed state', () => {
    expect(() => parseJobView(rawJobView({ state: 'exploded' }))).toThrowError(
      /Malformed job view/,
    );
  });

  it('rejects a missing required field', () => {
    expect(() => parseJobView(rawJobView({ jobId: undefined }))).toThrowError(/Malformed job view/);
  });
});

describe('parseCancelReceipt', () => {
  it('parses a valid receipt', () => {
    expect(
      parseCancelReceipt({ executionId: 'exec-1', disposition: 'requested', state: 'cancelled' }),
    ).toEqual({ executionId: 'exec-1', disposition: 'requested', state: 'cancelled' });
  });

  it('rejects a bad disposition', () => {
    expect(() =>
      parseCancelReceipt({ executionId: 'exec-1', disposition: 'maybe', state: 'cancelled' }),
    ).toThrowError(/Malformed cancel receipt/);
  });
});

describe('BackendClient job methods against a fake transport', () => {
  beforeEach(() => clearBackendClients());
  afterEach(() => clearBackendClients());

  it('startJob records the receipt and getJob/listJobs parse the DTO', async () => {
    const fake = createFakeTransport();
    fake.responses.set('startJob', rawJobView());
    fake.responses.set('getJob', rawJobView({ state: 'succeeded' }));
    fake.responses.set('listJobs', [rawJobView(), rawJobView({ jobId: 'job-9' })]);

    const client = createBackendClient('desktop', fake.transport);
    const job = await client.startJob({ kind: 'schemaDiffApply' }, TOKEN);
    expect(job.jobId).toBe('job-1');
    expect(fake.calls[0]?.payload).toEqual({ kind: 'schemaDiffApply', submissionToken: TOKEN });

    const fetched = await client.getJob(id('job-1'));
    expect(fetched.state).toBe('succeeded');

    const listed = await client.listJobs({ states: ['running'] });
    expect(listed).toHaveLength(2);
    expect(listed[1]?.jobId).toBe('job-9');
  });

  it('cancelJob returns a parsed receipt', async () => {
    const fake = createFakeTransport();
    fake.responses.set('cancelJob', {
      executionId: 'exec-1',
      disposition: 'requested',
      state: 'cancelRequested',
    });

    const client = createBackendClient('desktop', fake.transport);
    await expect(client.cancelJob(id('job-1'), TOKEN)).resolves.toEqual({
      executionId: 'exec-1',
      disposition: 'requested',
      state: 'cancelRequested',
    });
  });

  it('watchJob re-subscribes after an error and stop() is unsubscribe-only', async () => {
    const fake = createFakeTransport();
    fake.responses.set('getJob', rawJobView());
    fake.failures.set('getJob', { code: 'ServiceUnavailable', message: 'lost', requestId: 'r-1' });

    const client = createBackendClient('desktop', fake.transport);
    const updates: Array<{ resubscribed: boolean }> = [];
    const errors: unknown[] = [];
    const watch = client.watchJob(id('job-1'), (_view, meta) => updates.push(meta), {
      intervalMs: 5,
      onError: (e) => errors.push(e),
    });

    await new Promise((resolve) => setTimeout(resolve, 30));
    watch.stop();

    expect(errors.length).toBeGreaterThan(0);
    expect(updates.length).toBeGreaterThan(0);
    // The first delivery after any error is the re-subscription.
    expect(updates.some((u) => u.resubscribed === true)).toBe(true);
    // Unsubscribe only: no backend cancel is issued for the job.
    expect(fake.calls.filter((c) => c.method === 'cancelJob')).toHaveLength(0);
  });

  it('submitJobIdempotent recovers from the original receipt on timeout', async () => {
    const fake = createFakeTransport();
    fake.responses.set('startJob', rawJobView({ state: 'queued' }));
    fake.responses.set('getJob', rawJobView({ state: 'succeeded' }));
    fake.responses.set('listJobs', []);

    const client = createBackendClient('desktop', fake.transport);
    // Prime the receipt map with one accepted submission.
    await client.startJob({ kind: 'schemaDiffApply' }, TOKEN);

    // Next submission of the same kind times out: the client must consult
    // the original receipt instead of issuing a second apply.
    fake.responses.set('startJob', new Promise(() => undefined));
    const recovered = await client.submitJobIdempotent({ kind: 'schemaDiffApply' }, TOKEN, {
      timeoutMs: 10,
    });
    expect(recovered.state).toBe('succeeded');
    expect(fake.calls.filter((c) => c.method === 'startJob')).toHaveLength(2);
  });

  it('submitJobIdempotent never issues a second startJob for an unknown commit', async () => {
    const fake = createFakeTransport();
    fake.responses.set('startJob', new Promise(() => undefined));
    fake.responses.set('listJobs', []);

    const client = createBackendClient('desktop', fake.transport);
    await expect(
      client.submitJobIdempotent({ kind: 'dataSyncApply' }, TOKEN, { timeoutMs: 10 }),
    ).rejects.toThrowError(/timed out/);
    expect(fake.calls.filter((c) => c.method === 'startJob')).toHaveLength(1);
  });

  it('submitJobIdempotent returns the single recent job when one matches', async () => {
    const fake = createFakeTransport();
    fake.responses.set('startJob', new Promise(() => undefined));
    fake.responses.set('listJobs', [rawJobView({ kind: 'dataTransferApply', state: 'running' })]);

    const client = createBackendClient('desktop', fake.transport);
    const recovered = await client.submitJobIdempotent({ kind: 'dataTransferApply' }, TOKEN, {
      timeoutMs: 10,
    });
    expect(recovered.jobId).toBe('job-1');
  });
});
