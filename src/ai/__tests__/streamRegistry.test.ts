import { describe, expect, it } from 'vitest';
import { StreamRegistry } from '../streamRegistry';

const CONV = 'conv-1';

describe('StreamRegistry lifecycle', () => {
  it('walks starting → streaming → terminal(done) step by step', () => {
    const registry = new StreamRegistry();
    const started = registry.begin(CONV, 'req-1');
    expect(started.phase).toBe('starting');
    expect(started.generation).toBe(1);
    expect(started.terminalKind).toBeNull();

    // First non-terminal chunk enters streaming.
    const first = registry.applyChunk('req-1', {});
    expect(first).toMatchObject({ accepted: true, phase: 'streaming', generation: 1 });
    expect(registry.get('req-1')?.phase).toBe('streaming');

    // Subsequent chunks stay streaming.
    expect(registry.applyChunk('req-1', {}).phase).toBe('streaming');

    // Terminal chunk leaves the machine exactly once.
    const terminal = registry.applyChunk('req-1', { done: true });
    expect(terminal).toMatchObject({
      accepted: true,
      phase: 'terminal',
      terminalKind: 'done',
    });
    expect(registry.get('req-1')?.phase).toBe('terminal');
  });

  it('is terminal-idempotent: a second terminal chunk is dropped', () => {
    const registry = new StreamRegistry();
    registry.begin(CONV, 'req-1');
    expect(registry.applyChunk('req-1', { done: true }).accepted).toBe(true);

    const again = registry.applyChunk('req-1', { done: true });
    expect(again.accepted).toBe(false);
    expect(again.dropReason).toBe('terminal');
    expect(registry.get('req-1')?.terminalKind).toBe('done');
  });

  it('treats done && cancelled as terminal(cancelled), not done', () => {
    const registry = new StreamRegistry();
    registry.begin(CONV, 'req-1');
    registry.applyChunk('req-1', {});

    const result = registry.applyChunk('req-1', { done: true, cancelled: true });
    expect(result.accepted).toBe(true);
    expect(result.phase).toBe('terminal');
    expect(result.terminalKind).toBe('cancelled');
  });

  it('drops non-terminal chunks after the generation changes', () => {
    const registry = new StreamRegistry();
    registry.begin(CONV, 'req-1');
    registry.applyChunk('req-1', {});

    // A new run supersedes req-1 and bumps the generation.
    const second = registry.begin(CONV, 'req-2');
    expect(second.generation).toBe(2);
    expect(registry.conversationGeneration(CONV)).toBe(2);
    expect(registry.get('req-1')?.phase).toBe('terminal');
    expect(registry.get('req-1')?.terminalKind).toBe('cancelled');

    const late = registry.applyChunk('req-1', {});
    expect(late.accepted).toBe(false);
    expect(late.dropReason).toBe('stale-generation');

    // The new run is unaffected and starts in its own starting phase.
    expect(registry.applyChunk('req-2', {}).accepted).toBe(true);
    expect(registry.get('req-2')?.phase).toBe('streaming');
  });

  it('keeps generations monotonic across many runs of one conversation', () => {
    const registry = new StreamRegistry();
    registry.begin(CONV, 'r1');
    registry.begin(CONV, 'r2');
    registry.begin(CONV, 'r3');
    expect(registry.conversationGeneration(CONV)).toBe(3);
    expect(registry.get('r3')?.generation).toBe(3);

    // A different conversation has its own independent generation series.
    registry.begin('conv-2', 'other');
    expect(registry.conversationGeneration('conv-2')).toBe(1);
    expect(registry.conversationGeneration(CONV)).toBe(3);
  });

  it('drops late content chunks after cancel is marked but accepts the terminal one', () => {
    const registry = new StreamRegistry();
    registry.begin(CONV, 'req-1');
    registry.applyChunk('req-1', {});

    expect(registry.markCancelling('req-1', 1234)).toBe(true);
    const cancelling = registry.get('req-1');
    expect(cancelling?.phase).toBe('cancelling');
    expect(cancelling?.cancelIssuedAtMs).toBe(1234);

    const late = registry.applyChunk('req-1', {});
    expect(late.accepted).toBe(false);
    expect(late.dropReason).toBe('cancel-issued');
    expect(registry.get('req-1')?.phase).toBe('cancelling');

    const terminal = registry.applyChunk('req-1', { done: true, cancelled: true });
    expect(terminal.accepted).toBe(true);
    expect(terminal.terminalKind).toBe('cancelled');

    // The fallback settle after a real terminal chunk is a no-op.
    expect(registry.settle('req-1', 'cancelled')).toBe(false);
  });

  it('accepts a normally-finished terminal chunk while cancelling', () => {
    const registry = new StreamRegistry();
    registry.begin(CONV, 'req-1');
    registry.applyChunk('req-1', {});
    registry.markCancelling('req-1');
    expect(registry.applyChunk('req-1', { done: true }).terminalKind).toBe('done');
  });

  it('settles on an out-of-band error and rejects later chunks', () => {
    const registry = new StreamRegistry();
    registry.begin(CONV, 'req-1');
    registry.applyChunk('req-1', {});

    const error = registry.applyError('req-1');
    expect(error.accepted).toBe(true);
    expect(error.terminalKind).toBe('error');
    expect(registry.applyError('req-1').dropReason).toBe('terminal');
    expect(registry.applyChunk('req-1', { done: true }).dropReason).toBe('terminal');
  });

  it('rejects chunks for unknown requests', () => {
    const registry = new StreamRegistry();
    const result = registry.applyChunk('missing', { done: true });
    expect(result.accepted).toBe(false);
    expect(result.dropReason).toBe('unknown-request');
    expect(result.phase).toBeNull();
  });

  it('force-settles a stuck cancelling run (8s fallback path)', () => {
    const registry = new StreamRegistry();
    registry.begin(CONV, 'req-1');
    registry.applyChunk('req-1', {});
    registry.markCancelling('req-1');
    expect(registry.get('req-1')?.phase).toBe('cancelling');

    expect(registry.settle('req-1', 'cancelled')).toBe(true);
    expect(registry.get('req-1')?.phase).toBe('terminal');
    expect(registry.get('req-1')?.terminalKind).toBe('cancelled');
    expect(registry.applyChunk('req-1', { done: true, cancelled: true }).dropReason).toBe(
      'terminal',
    );
  });

  it('emits started/streaming/cancelling/settled events in order', () => {
    const registry = new StreamRegistry();
    const events: string[] = [];
    const unsubscribe = registry.subscribe((event) => {
      events.push(event.type === 'settled' ? `settled:${event.terminalKind}` : event.type);
    });

    registry.begin(CONV, 'req-1');
    registry.applyChunk('req-1', {});
    registry.markCancelling('req-1');
    registry.applyChunk('req-1', { done: true, cancelled: true });

    expect(events).toEqual(['started', 'streaming', 'cancelling', 'settled:cancelled']);
    unsubscribe();
  });

  it('does not supersede the same live request when begin() is repeated', () => {
    const registry = new StreamRegistry();
    const first = registry.begin(CONV, 'req-1');
    registry.applyChunk('req-1', {});

    const duplicate = registry.begin(CONV, 'req-1');
    expect(duplicate.generation).toBe(first.generation);
    expect(duplicate.phase).toBe('streaming');
    expect(registry.conversationGeneration(CONV)).toBe(1);
  });
});
