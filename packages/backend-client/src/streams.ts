/**
 * Event streams — the frontend's only view of backend pushes.
 *
 * The contract (`docs/architecture/platform/connection-management.md` §4.1)
 * is an `AsyncIterable<EventEnvelope<ConnectionEvent>>`. This module supplies
 * that abstraction in a form that cannot be got wrong:
 *
 * **Ending iteration cancels the subscription, and nothing else.** A `break`
 * out of `for await` closes the local subscription. It does *not* call
 * `cancelExecution`, and it must not: the user leaving the events panel is not
 * the user cancelling the SQL. Only an explicit cancel control issues
 * `cancelExecution`. This is the rule most easily broken by an eager cleanup
 * helper, so it is enforced structurally here — see `createEventStream`.
 */

import type { ConnectionEvent, EventEnvelope } from './types';

/**
 * A subscription over backend events.
 *
 * Iteration order is by ascending `sequence`; duplicates and gaps are the
 * consumer's to handle (ignore repeats, re-read state on a gap).
 */
export interface EventStream<T> extends AsyncIterable<T> {
  /** Id of the backend stream this subscription reads. */
  readonly streamId: string;
  /** Highest sequence delivered so far, or `null` before the first event. */
  lastSequence(): number | null;
  /** Stop the subscription. Idempotent; never cancels backend-side work. */
  close(): Promise<void>;
}

/**
 * Pull side of a subscription, implemented by the adapter.
 *
 * Deliberately *pull*-based: a push adapter (an IPC channel, a socket feed)
 * buffers and fulfils `next`. That keeps this module free of any transport
 * type while still giving adapters a single, easy-to-satisfy obligation.
 */
export interface EventStreamSource<T> {
  /**
   * Await the next delivery after `afterSequence`, or return `undefined` when
   * the backend closed the stream.
   *
   * Passing the last delivered sequence is what makes a dropped subscription
   * resumable: the backend replays from `afterSequence + 1` instead of the
   * client guessing where it left off.
   */
  next(afterSequence: number | null): Promise<T | undefined>;
  /**
   * Stop producing. Must release local resources only — the backend-side
   * execution, if any, keeps running and stays observable via `getExecution`.
   */
  release(): Promise<void> | void;
}

/**
 * Build an `EventStream` from an adapter-provided source.
 *
 * `release()` runs exactly once, whether the stream ended normally, was closed
 * explicitly, or was abandoned by `break`/`return`/`throw`.
 */
export function createEventStream<T>(
  source: EventStreamSource<T>,
  streamId: string,
): EventStream<T> {
  let sequence: number | null = null;
  let released = false;

  async function release(): Promise<void> {
    if (released) return;
    released = true;
    await source.release();
  }

  async function* iterate(): AsyncGenerator<T, void, undefined> {
    try {
      for (;;) {
        const event = released ? undefined : await source.next(sequence);
        if (event === undefined) return;
        const delivered = readSequence(event);
        if (delivered !== null) sequence = delivered;
        yield event;
      }
    } finally {
      // `break`, `return` and an exception inside the loop body all land here,
      // which is what guarantees a subscription is never left dangling.
      await release();
    }
  }

  return {
    streamId,
    lastSequence: () => sequence,
    close: release,
    [Symbol.asyncIterator]: () => iterate(),
  };
}

/**
 * Adapt an adapter-supplied `AsyncIterable` into an `EventStream`.
 *
 * This is the seam §7.2 asks for: a push adapter (an IPC channel, a socket
 * feed) hands over an iterable, and `driverCommands.executeStream`'s `onEvent`
 * callback is fed by iterating the returned `EventStream`.
 *
 * `next(afterSequence)` drops anything already delivered before `afterSequence`,
 * so a reconnect that replays the tail of the feed does not re-deliver events
 * the consumer has seen.
 */
export function toEventStream<T>(iterable: AsyncIterable<T>, streamId: string): EventStream<T> {
  const iterator = iterable[Symbol.asyncIterator]();
  let ended = false;

  const source: EventStreamSource<T> = {
    async next(afterSequence: number | null): Promise<T | undefined> {
      for (;;) {
        if (ended) return undefined;

        const step = await iterator.next();
        if (step.done) {
          ended = true;
          return undefined;
        }
        if (afterSequence !== null) {
          const seen = readSequence(step.value);
          if (seen !== null && seen <= afterSequence) continue;
        }
        return step.value;
      }
    },
    async release(): Promise<void> {
      if (ended) return;
      ended = true;
      // Only the subscription is torn down here. Whatever produced it — a
      // running execution included — is untouched.
      await iterator.return?.();
    },
  };

  return createEventStream(source, streamId);
}

/**
 * Track the delivered sequence number.
 *
 * `EventEnvelope.sequence` is a branded `Counter`, so this only has to narrow
 * it back to a plain number for the adapter-facing source signature. Unknown
 * envelopes keep the previous value: skipping the update is recoverable, while
 * inventing a sequence number would corrupt resumption.
 */
function readSequence(event: unknown): number | null {
  if (typeof event !== 'object' || event === null) return null;
  const candidate = (event as { sequence?: unknown }).sequence;
  return typeof candidate === 'number' ? candidate : null;
}

/** Concrete envelope type used by `BackendClient.subscribeEvents`. */
export type ConnectionEventStream = EventStream<EventEnvelope<ConnectionEvent>>;
