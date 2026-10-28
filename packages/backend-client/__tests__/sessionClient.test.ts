import { describe, expect, it } from 'vitest';

import {
  ApiError,
  createBackendClient,
  toId,
  toTimestamp,
  type OpenSessionRequest,
  type SubmissionToken,
} from '../src/index';
import { createFakeTransport, TEST_HANDLE } from './client.test';

/**
 * Session domain wire contracts, migrated from the pre-F3 facade suite: the
 * method name on the wire, the exact envelope shape, and error normalization.
 */

const TOKEN: SubmissionToken = {
  idempotencyKey: 'idem-1',
  expiresAt: mustNarrow(toTimestamp(1_700_000_000_000)),
};

const OPEN_REQUEST: OpenSessionRequest = {
  initialTarget: {
    connectionId: mustNarrow(toId('conn-1')),
    namespace: { database: null, catalog: null, schema: 'public', path: [] },
    object: null,
  },
  owner: {
    kind: 'editor',
    clientInstanceId: mustNarrow(toId('client-1')),
    editorSessionId: mustNarrow(toId('ed-1')),
  },
  idempotencyKey: 'idem-open-1',
};

function mustNarrow<T>(narrowed: T | undefined, value?: unknown): T {
  if (narrowed === undefined) throw new Error(`fixture ${String(value)} did not narrow`);
  return narrowed;
}

describe('Session domain client', () => {
  it('sends the documented method name and request for openSession', async () => {
    const fake = createFakeTransport();
    fake.responses.set('openSession', { session: TEST_HANDLE });

    const client = createBackendClient('desktop', fake.transport);
    await client.openSession(OPEN_REQUEST, TOKEN);

    expect(fake.calls).toHaveLength(1);
    expect(fake.calls[0]?.method).toBe('openSession');
    // The token is folded into the request rather than carried as a separate
    // transport argument, so the adapter sees exactly one envelope.
    expect(fake.calls[0]?.payload).toEqual({ ...OPEN_REQUEST, submissionToken: TOKEN });
  });

  it('returns the transport response unchanged', async () => {
    const fake = createFakeTransport();
    const receipt = { session: TEST_HANDLE };
    fake.responses.set('openSession', receipt);

    const client = createBackendClient('desktop', fake.transport);
    await expect(client.openSession(OPEN_REQUEST, TOKEN)).resolves.toBe(receipt);
  });

  it('reads a session by handle without adding anything to the payload', async () => {
    const fake = createFakeTransport();
    fake.responses.set('getSession', {});

    const client = createBackendClient('desktop', fake.transport);
    await client.getSession(TEST_HANDLE);

    expect(fake.calls[0]).toEqual({ method: 'getSession', payload: { handle: TEST_HANDLE } });
  });

  it('turns a transport rejection into an ApiError carrying the requestId', async () => {
    const fake = createFakeTransport();
    fake.transport.call = () =>
      Promise.reject({ code: 'SessionNotFound', message: 'gone', requestId: 'r-1' }) as never;
    const client = createBackendClient('desktop', fake.transport);

    await expect(client.getSession(TEST_HANDLE)).rejects.toMatchObject({
      code: 'SessionNotFound',
      requestId: 'r-1',
    });
    await expect(client.getSession(TEST_HANDLE)).rejects.toBeInstanceOf(ApiError);
  });

  it('does not swallow an ApiError the transport already produced', async () => {
    const original = new ApiError('ContextConflict', 'context moved on', { requestId: 'r-2' });
    const fake = createFakeTransport();
    fake.transport.call = () => Promise.reject(original) as never;
    const client = createBackendClient('desktop', fake.transport);

    await expect(client.getSession(TEST_HANDLE)).rejects.toBe(original);
  });
});
