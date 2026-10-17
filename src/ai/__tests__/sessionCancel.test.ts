import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { CANCEL_TIMEOUT_MS, SessionCanceller } from '../sessionCancel';
import { StreamRegistry } from '../streamRegistry';
import type { CancelReceipt } from '../targetClaim';

function marked(requestId = 'req-1'): CancelReceipt {
  return { kind: 'marked', requestId };
}

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
  vi.clearAllMocks();
});

describe('SessionCanceller', () => {
  it('marked → cancelling, then the terminal chunk clears the 8s fallback', async () => {
    const registry = new StreamRegistry();
    registry.begin('conv', 'req-1');
    registry.applyChunk('req-1', {});
    const onCancelTimeout = vi.fn();
    const canceller = new SessionCanceller({
      registry,
      cancel: vi.fn(async () => marked()),
      onCancelTimeout,
    });

    const receipt = await canceller.cancel('req-1');
    expect(receipt).toEqual(marked());
    expect(registry.get('req-1')?.phase).toBe('cancelling');

    // The backend delivers its terminal chunk before the fallback fires.
    registry.applyChunk('req-1', { done: true, cancelled: true });
    expect(registry.get('req-1')?.terminalKind).toBe('cancelled');

    await vi.advanceTimersByTimeAsync(CANCEL_TIMEOUT_MS);
    expect(onCancelTimeout).not.toHaveBeenCalled();
    canceller.dispose();
  });

  it('forces terminal(cancelled) after 8s and finalises the UI', async () => {
    const registry = new StreamRegistry();
    registry.begin('conv', 'req-1');
    registry.applyChunk('req-1', {});
    const onCancelTimeout = vi.fn();
    const canceller = new SessionCanceller({
      registry,
      cancel: vi.fn(async () => marked()),
      onCancelTimeout,
    });

    await canceller.cancel('req-1');
    await vi.advanceTimersByTimeAsync(CANCEL_TIMEOUT_MS - 1);
    expect(registry.get('req-1')?.phase).toBe('cancelling');
    expect(onCancelTimeout).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(1);
    expect(registry.get('req-1')?.phase).toBe('terminal');
    expect(registry.get('req-1')?.terminalKind).toBe('cancelled');
    expect(onCancelTimeout).toHaveBeenCalledWith('req-1');
    canceller.dispose();
  });

  it('forbidden only warns and leaves the stream untouched', async () => {
    const registry = new StreamRegistry();
    registry.begin('conv', 'req-1');
    registry.applyChunk('req-1', {});
    const onWarn = vi.fn();
    const onCancelTimeout = vi.fn();
    const canceller = new SessionCanceller({
      registry,
      cancel: vi.fn(async () => ({ kind: 'forbidden', requestId: 'req-1' }) as const),
      onWarn,
      onCancelTimeout,
    });

    const receipt = await canceller.cancel('req-1');
    expect(receipt?.kind).toBe('forbidden');
    expect(onWarn).toHaveBeenCalled();
    expect(registry.get('req-1')?.phase).toBe('streaming');

    await vi.advanceTimersByTimeAsync(CANCEL_TIMEOUT_MS);
    expect(onCancelTimeout).not.toHaveBeenCalled();
    canceller.dispose();
  });

  it('alreadyFinished does not enter cancelling and arms no fallback', async () => {
    const registry = new StreamRegistry();
    registry.begin('conv', 'req-1');
    registry.applyChunk('req-1', {});
    const onCancelTimeout = vi.fn();
    const canceller = new SessionCanceller({
      registry,
      cancel: vi.fn(async () => ({ kind: 'alreadyFinished', requestId: 'req-1' }) as const),
      onCancelTimeout,
    });

    const receipt = await canceller.cancel('req-1');
    expect(receipt?.kind).toBe('alreadyFinished');
    expect(registry.get('req-1')?.phase).toBe('streaming');

    await vi.advanceTimersByTimeAsync(CANCEL_TIMEOUT_MS);
    expect(onCancelTimeout).not.toHaveBeenCalled();
    canceller.dispose();
  });

  it('a failed ai_cancel IPC warns and leaves the run streaming', async () => {
    const registry = new StreamRegistry();
    registry.begin('conv', 'req-1');
    registry.applyChunk('req-1', {});
    const onWarn = vi.fn();
    const canceller = new SessionCanceller({
      registry,
      cancel: vi.fn(async () => {
        throw new Error('ipc down');
      }),
      onWarn,
    });

    const receipt = await canceller.cancel('req-1');
    expect(receipt).toBeNull();
    expect(onWarn).toHaveBeenCalled();
    expect(registry.get('req-1')?.phase).toBe('streaming');
    canceller.dispose();
  });

  it('marking an already-terminal request does not arm a fallback', async () => {
    const registry = new StreamRegistry();
    registry.begin('conv', 'req-1');
    registry.applyChunk('req-1', { done: true });
    const onCancelTimeout = vi.fn();
    const canceller = new SessionCanceller({
      registry,
      cancel: vi.fn(async () => marked()),
      onCancelTimeout,
    });

    await canceller.cancel('req-1');
    await vi.advanceTimersByTimeAsync(CANCEL_TIMEOUT_MS);
    expect(onCancelTimeout).not.toHaveBeenCalled();
    canceller.dispose();
  });
});
