import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { GrantState, type GrantInput, type GrantStateDeps } from '../grantState';
import type { AiGrantReceipt } from '../targetClaim';

const BASE = new Date('2026-01-01T00:00:00.000Z').getTime();
const TTL = 10_000;

const INPUT: GrantInput = {
  connectionId: 'conn-1',
  database: 'db',
  editorSessionId: 'panel-1',
};

function receipt(grantId: string, ttl = TTL): AiGrantReceipt {
  return {
    grantId,
    connectionId: INPUT.connectionId,
    database: INPUT.database,
    configRevision: 3,
    expiresAtMs: Date.now() + ttl,
  };
}

function makeDeps(overrides: Partial<GrantStateDeps> = {}) {
  const grant = vi.fn(async () => receipt('grant-1'));
  const revoke = vi.fn(async () => true);
  const touch = vi.fn(async () => true);
  const deps: GrantStateDeps = { grant, revoke, touch, ...overrides };
  return { deps, grant, revoke, touch };
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(BASE);
});

afterEach(() => {
  vi.useRealTimers();
  vi.clearAllMocks();
});

describe('GrantState lifecycle', () => {
  it('walks independent → granting → granted and touches at ttl/2', async () => {
    const { deps, touch } = makeDeps();
    const state = new GrantState(deps);
    expect(state.snapshot.phase).toBe('independent');
    expect(state.snapshot.receipt).toBeNull();

    const pending = state.grant(INPUT);
    // Entry condition is observable synchronously, before the host answers.
    expect(state.snapshot.phase).toBe('granting');
    expect(state.snapshot.error).toBeNull();

    await pending;
    expect(state.snapshot.phase).toBe('granted');
    expect(state.snapshot.receipt?.grantId).toBe('grant-1');
    expect(state.snapshot.expiresAtMs).toBe(BASE + TTL);

    // Nothing is touched before half the ttl has elapsed.
    expect(touch).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(TTL / 2 - 1);
    expect(touch).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(1);
    expect(touch).toHaveBeenCalledTimes(1);
    expect(touch).toHaveBeenCalledWith('grant-1');
    // A successful touch extends the local deadline by one ttl.
    expect(state.snapshot.expiresAtMs).toBe(BASE + TTL / 2 + TTL);

    // The next touch is scheduled relative to the extended deadline.
    await vi.advanceTimersByTimeAsync(TTL / 2);
    expect(touch).toHaveBeenCalledTimes(2);

    state.dispose();
  });

  it('returns to independent when the local expiresAtMs deadline is reached', async () => {
    const { deps, grant } = makeDeps({
      // A touch that never answers: only the local deadline can end the grant.
      touch: vi.fn(() => new Promise<boolean>(() => {})),
    });
    const state = new GrantState(deps);
    await state.grant(INPUT);
    expect(state.snapshot.phase).toBe('granted');

    await vi.advanceTimersByTimeAsync(TTL / 2);
    await vi.advanceTimersByTimeAsync(TTL / 2);

    expect(state.snapshot.phase).toBe('independent');
    expect(state.snapshot.receipt).toBeNull();
    expect(state.snapshot.expiresAtMs).toBeNull();

    // Exit is not terminal: a later grant can start over from independent.
    grant.mockResolvedValueOnce(receipt('grant-2'));
    await state.grant(INPUT);
    expect(state.snapshot.phase).toBe('granted');
    expect(state.snapshot.receipt?.grantId).toBe('grant-2');
    state.dispose();
  });

  it('moves to revoked when a scheduled touch fails, and can be granted again', async () => {
    const { deps, grant, touch } = makeDeps();
    touch.mockResolvedValue(false);
    const state = new GrantState(deps);
    await state.grant(INPUT);

    await vi.advanceTimersByTimeAsync(TTL / 2);

    expect(touch).toHaveBeenCalledWith('grant-1');
    expect(state.snapshot.phase).toBe('revoked');
    expect(state.snapshot.receipt).toBeNull();
    expect(state.snapshot.error).toBeTruthy();

    // Revoked is an exit state, not a dead end.
    grant.mockResolvedValueOnce(receipt('grant-2'));
    const regrant = state.grant(INPUT);
    expect(state.snapshot.phase).toBe('granting');
    await regrant;
    expect(state.snapshot.phase).toBe('granted');
    expect(state.snapshot.receipt?.grantId).toBe('grant-2');
    state.dispose();
  });

  it('moves to revoked when the touch IPC rejects', async () => {
    const { deps } = makeDeps({
      touch: vi.fn(async () => {
        throw new Error('transport closed');
      }),
    });
    const state = new GrantState(deps);
    await state.grant(INPUT);

    await vi.advanceTimersByTimeAsync(TTL / 2);
    expect(state.snapshot.phase).toBe('revoked');
    expect(state.snapshot.error).toContain('transport closed');
    state.dispose();
  });

  it('a denied grant leaves the panel independent with a visible error', async () => {
    const { deps, grant } = makeDeps();
    grant.mockRejectedValueOnce(new Error('target not permitted'));
    const state = new GrantState(deps);

    const result = await state.grant(INPUT);

    expect(result).toBeNull();
    expect(state.snapshot.phase).toBe('independent');
    expect(state.snapshot.receipt).toBeNull();
    expect(state.snapshot.error).toContain('target not permitted');
  });

  it('revoke clears timers, releases the grant and blocks future touches', async () => {
    const { deps, revoke, touch } = makeDeps();
    const state = new GrantState(deps);
    await state.grant(INPUT);

    await state.revoke();

    expect(state.snapshot.phase).toBe('revoked');
    expect(state.snapshot.receipt).toBeNull();
    expect(revoke).toHaveBeenCalledWith('grant-1');

    await vi.advanceTimersByTimeAsync(TTL * 3);
    expect(touch).not.toHaveBeenCalled();
  });

  it('revoking while granting releases the late receipt instead of keeping it', async () => {
    const { deps, grant, revoke } = makeDeps();
    let resolveGrant: (value: AiGrantReceipt) => void = () => {};
    grant.mockImplementationOnce(
      () =>
        new Promise<AiGrantReceipt>((resolve) => {
          resolveGrant = resolve;
        }),
    );
    const state = new GrantState(deps);

    const pending = state.grant(INPUT);
    expect(state.snapshot.phase).toBe('granting');

    await state.revoke();
    expect(state.snapshot.phase).toBe('revoked');

    resolveGrant(receipt('grant-late'));
    await pending;
    await Promise.resolve();

    expect(state.snapshot.phase).toBe('revoked');
    expect(state.snapshot.receipt).toBeNull();
    expect(revoke).toHaveBeenCalledWith('grant-late');
  });

  it('dispose releases an active grant and clears the touch timer', async () => {
    const { deps, revoke, touch } = makeDeps();
    const state = new GrantState(deps);
    await state.grant(INPUT);

    state.dispose();

    expect(revoke).toHaveBeenCalledWith('grant-1');
    await vi.advanceTimersByTimeAsync(TTL * 2);
    expect(touch).not.toHaveBeenCalled();
  });

  it('does not touch after a local revoke race during a pending touch', async () => {
    const { deps, touch } = makeDeps();
    let resolveTouch: (value: boolean) => void = () => {};
    touch.mockImplementationOnce(
      () =>
        new Promise<boolean>((resolve) => {
          resolveTouch = resolve;
        }),
    );
    const state = new GrantState(deps);
    await state.grant(INPUT);

    await vi.advanceTimersByTimeAsync(TTL / 2);
    await state.revoke();
    resolveTouch(true);
    await Promise.resolve();
    await Promise.resolve();

    expect(state.snapshot.phase).toBe('revoked');
    expect(state.snapshot.receipt).toBeNull();
  });

  it('reports transitions through subscribe and stops after unsubscribe', async () => {
    const { deps } = makeDeps();
    const state = new GrantState(deps);
    const phases: string[] = [];
    const unsubscribe = state.subscribe((snapshot) => phases.push(snapshot.phase));

    await state.grant(INPUT);
    await state.revoke();
    unsubscribe();
    await state.grant(INPUT);
    await state.revoke();

    expect(phases).toContain('granting');
    expect(phases).toContain('granted');
    expect(phases).toContain('revoked');
  });
});