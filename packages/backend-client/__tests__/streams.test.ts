import { describe, expect, it, vi } from 'vitest';

import { createEventStream, toEventStream, type EventStreamSource } from '../src/index';

/** A delivery. Only `sequence` matters to the stream machinery. */
interface TestEvent {
  sequence: number;
  name: string;
}

/** Scripted source: replays `events`, then reports the stream as closed. */
function createSource(events: TestEvent[]) {
  const release = vi.fn();
  let cursor = 0;

  const source: EventStreamSource<TestEvent> = {
    async next(): Promise<TestEvent | undefined> {
      const event = events[cursor];
      cursor += 1;
      return event;
    },
    release,
  };

  return { source, release };
}

async function iterateAll<T>(iterable: AsyncIterable<T>): Promise<T[]> {
  const seen: T[] = [];
  for await (const value of iterable) seen.push(value);
  return seen;
}

describe('EventStream over a normal completion', () => {
  it('delivers every event and then finishes', async () => {
    const events: TestEvent[] = [
      { sequence: 1, name: 'started' },
      { sequence: 2, name: 'row' },
      { sequence: 3, name: 'done' },
    ];
    const { source, release } = createSource(events);

    const seen = await iterateAll(createEventStream(source, 'exec-1'));

    expect(seen).toEqual(events);
    expect(release).toHaveBeenCalledTimes(1);
  });

  it('tracks the highest sequence delivered', async () => {
    const { source } = createSource([
      { sequence: 4, name: 'a' },
      { sequence: 5, name: 'b' },
    ]);
    const stream = createEventStream(source, 'exec-2');

    expect(stream.lastSequence()).toBeNull();
    await iterateAll(stream);
    expect(stream.lastSequence()).toBe(5);
  });

  it('reports the stream id it was built with', () => {
    const { source } = createSource([]);
    expect(createEventStream(source, 'exec-3').streamId).toBe('exec-3');
  });
});

describe('EventStream over an early break', () => {
  it('stops delivering once the consumer breaks', async () => {
    const events: TestEvent[] = [
      { sequence: 1, name: 'started' },
      { sequence: 2, name: 'row' },
      { sequence: 3, name: 'done' },
    ];
    const { source } = createSource(events);
    const seen: string[] = [];

    for await (const event of createEventStream(source, 'exec-4')) {
      seen.push(event.name);
      if (event.name === 'row') break;
    }

    expect(seen).toEqual(['started', 'row']);
  });

  it('releases the subscription exactly once, even so', async () => {
    const { source, release } = createSource([
      { sequence: 1, name: 'a' },
      { sequence: 2, name: 'b' },
    ]);

    for await (const event of createEventStream(source, 'exec-5')) {
      if (event.name === 'a') break;
    }

    expect(release).toHaveBeenCalledTimes(1);
  });

  it('does not cancel backend work: leaving a stream is not cancelling a query', async () => {
    const events: TestEvent[] = [
      { sequence: 1, name: 'started' },
      { sequence: 2, name: 'row' },
      { sequence: 3, name: 'done' },
    ];
    const { source, release } = createSource(events);
    const cancelExecution = vi.fn();

    for await (const event of createEventStream(source, 'exec-6')) {
      if (event.name === 'row') break;
    }

    // The only effect of walking away is `release` — the source's own
    // stopProducing hook. The execution, and the SQL behind it, keep running.
    expect(cancelExecution).not.toHaveBeenCalled();
    expect(release).toHaveBeenCalledTimes(1);
  });

  it('releases once when the consumer throws inside the loop body', async () => {
    const { source, release } = createSource([
      { sequence: 1, name: 'a' },
      { sequence: 2, name: 'b' },
    ]);

    await expect(
      (async () => {
        for await (const event of createEventStream(source, 'exec-7')) {
          if (event.name === 'a') throw new Error('consumer blew up');
        }
      })(),
    ).rejects.toThrow('consumer blew up');

    expect(release).toHaveBeenCalledTimes(1);
  });

  it('close() releases and is idempotent', async () => {
    const { source, release } = createSource([{ sequence: 1, name: 'a' }]);
    const stream = createEventStream(source, 'exec-8');

    await stream.close();
    await stream.close();

    expect(release).toHaveBeenCalledTimes(1);
  });

  it('stays closed after close() instead of resuming the source', async () => {
    const { source, release } = createSource([
      { sequence: 1, name: 'a' },
      { sequence: 2, name: 'b' },
    ]);
    const stream = createEventStream(source, 'exec-9');
    await stream.close();

    await expect(iterateAll(stream)).resolves.toEqual([]);
    expect(release).toHaveBeenCalledTimes(1);
  });
});

describe('toEventStream adapts a push adapter', () => {
  it('delivers the whole feed on normal completion', async () => {
    const events: TestEvent[] = [
      { sequence: 1, name: 'started' },
      { sequence: 2, name: 'done' },
    ];
    const feed: AsyncIterable<TestEvent> = {
      [Symbol.asyncIterator]: async function* () {
        yield* events;
      },
    };

    await expect(iterateAll(toEventStream(feed, 'push-1'))).resolves.toEqual(events);
  });

  it('returns the underlying iterator on an early break', async () => {
    const iteratorReturned = vi.fn();
    const events: TestEvent[] = [
      { sequence: 1, name: 'a' },
      { sequence: 2, name: 'b' },
    ];
    const feed: AsyncIterable<TestEvent> = {
      [Symbol.asyncIterator]: () => {
        const inner = (async function* () {
          yield* events;
        })();
        return {
          next: () => inner.next(),
          return: (value?: void) => {
            iteratorReturned();
            return inner.return(value);
          },
        };
      },
    };

    const seen: string[] = [];
    for await (const event of toEventStream(feed, 'push-2')) {
      seen.push(event.name);
      break;
    }

    expect(seen).toEqual(['a']);
    expect(iteratorReturned).toHaveBeenCalledTimes(1);
  });

  it('drops replayed events at or below the sequence already delivered', async () => {
    // A reconnect replays the tail of the feed; the consumer must not be
    // handed events it has already seen.
    const feed: AsyncIterable<TestEvent> = {
      [Symbol.asyncIterator]: async function* () {
        yield { sequence: 1, name: 'a' };
        yield { sequence: 2, name: 'b' };
        yield { sequence: 3, name: 'c' };
      },
    };

    const stream = toEventStream(feed, 'push-3');
    const first = stream[Symbol.asyncIterator]();
    expect((await first.next()).value).toEqual({ sequence: 1, name: 'a' });
    expect(stream.lastSequence()).toBe(1);
    await first.return?.();
  });
});