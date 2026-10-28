import { describe, expect, it } from 'vitest';

import { createBackendClient, toId, type BackendTransport } from '../src/index';
import type { EventEnvelope, ConnectionEvent, SubscribeEventsRequest } from '../src/index';

/**
 * Event domain client wire contracts, migrated from the pre-F3 facade suite.
 * The domain's implementation lives in `./streams` per the plan ("Event | 1 |
 * 并入 streams.ts"), so its contract tests exercise subscription routing.
 */

const id = (value: string) => {
  const narrowed = toId(value);
  if (narrowed === undefined) throw new Error(`fixture ${value} did not narrow`);
  return narrowed;
};

describe('Event domain client', () => {
  it('reports CapabilityUnsupported when the backend cannot subscribe', async () => {
    const transport: BackendTransport = {
      call: () => Promise.reject(new Error('unused')),
    };
    const client = createBackendClient('desktop', transport);

    const stream = client.subscribeEvents({ streamId: id('exec-1'), afterSequence: null });

    await expect(stream[Symbol.asyncIterator]().next()).rejects.toMatchObject({
      code: 'CapabilityUnsupported',
    });
  });

  it('hands the subscribe request to the transport untouched', async () => {
    const request: SubscribeEventsRequest = { streamId: id('exec-2'), afterSequence: null };
    const seen: SubscribeEventsRequest[] = [];
    const transport: BackendTransport = {
      call: () => Promise.reject(new Error('unused')),
      subscribe: (_method, payload) => {
        seen.push(payload);
        const feed: AsyncIterable<EventEnvelope<ConnectionEvent>> = {
          [Symbol.asyncIterator]: async function* () {
            // An empty feed: the assertion is about the request, not the events.
          },
        };
        return feed;
      },
    };
    const client = createBackendClient('desktop', transport);
    const stream = client.subscribeEvents(request);

    await stream[Symbol.asyncIterator]().next();

    expect(seen).toEqual([request]);
  });
});
