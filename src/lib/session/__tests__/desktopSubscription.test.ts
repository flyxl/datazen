import { afterEach, describe, expect, it, vi } from 'vitest';
import { createBackendClient } from '@datazen/backend-client';
import { ExecutionProjection } from '../ExecutionProjection';
import type { EventEnvelope, ConnectionEvent, Id } from '@datazen/backend-client';
const mock = vi.hoisted(() => ({ invoke: vi.fn(), channels: [] as { onmessage: (message: unknown) => void }[] }));
vi.mock('@tauri-apps/api/core', () => ({
  invoke: mock.invoke,
  Channel: class {
    onmessage = (_message: unknown) => {};
    constructor() { mock.channels.push(this); }
  },
}));
import { createDesktopBackendTransport } from '../../../platform/tauriBackendTransport';

afterEach(() => { vi.clearAllMocks(); mock.channels.length = 0; });
describe('desktop subscription adapter', () => {
  it('unblocks pending reads and only stops subscription on return', async () => {
    mock.invoke.mockResolvedValue(undefined);
    const transport = createDesktopBackendTransport();
    const stream = transport.subscribe?.('subscribeEvents', { streamId: 'stream' as Id, afterSequence: null });
    expect(stream).toBeDefined();
    const iterator = stream![Symbol.asyncIterator]();
    const event = { streamId: 'stream', sequence: '9007199254740994' } as unknown as EventEnvelope<ConnectionEvent>;
    const first = iterator.next();
    mock.channels[0].onmessage({ kind: 'event', event });
    expect((await first).value).toBe(event);
    const pending = iterator.next();
    await iterator.return?.();
    expect((await pending).done).toBe(true);
    expect(mock.invoke.mock.calls.map(([command]) => command)).toEqual(['subscribe_events', 'stop_event_subscription']);
    await iterator.return?.();
    expect(mock.invoke).toHaveBeenCalledTimes(2);
  });
  it('disposes a facade projection while its generator is waiting, without cancelling SQL', async () => {
    mock.invoke.mockResolvedValue(undefined);
    const client = createBackendClient('desktop', createDesktopBackendTransport());
    const projection = new ExecutionProjection(client, {
      executionId: 'e' as Id, streamId: 'stream' as Id, state: 'running',
    }, null);
    const consuming = projection.consume();
    await vi.waitFor(() => expect(mock.channels).toHaveLength(1));
    await projection.dispose();
    await consuming;
    expect(mock.invoke.mock.calls.map(([command]) => command)).toEqual(['subscribe_events', 'stop_event_subscription']);
  });
  it('does not start a subscription after immediate disposal during startup', async () => {
    mock.invoke.mockResolvedValue(undefined);
    const client = createBackendClient('desktop', createDesktopBackendTransport());
    const projection = new ExecutionProjection(client, {
      executionId: 'e' as Id, streamId: 'stream' as Id, state: 'running',
    }, null);
    const consuming = projection.consume();
    await projection.dispose();
    await consuming;
    expect(mock.invoke).not.toHaveBeenCalled();
  });
  it('projects stream errors and normalizes artifact bytes while preserving counters', async () => {
    mock.invoke.mockResolvedValue({ bytes: [1, 2], chunkIndex: '9007199254740993' });
    const transport = createDesktopBackendTransport();
    const chunk = await transport.call('readArtifact', { artifactId: 'a' as Id, chunkIndex: '0' as unknown as import('@datazen/backend-client').Counter });
    expect(chunk.bytes).toEqual(new Uint8Array([1, 2]));
    expect(chunk.chunkIndex).toBe('9007199254740993');
    expect(mock.invoke).toHaveBeenCalledWith('read_artifact', { request: { artifactId: 'a', chunkIndex: '0' } });
    mock.invoke.mockResolvedValue(undefined);
    const iterator = transport.subscribe!('subscribeEvents', { streamId: 'stream' as Id, afterSequence: null })[Symbol.asyncIterator]();
    const pending = iterator.next();
    mock.channels[0].onmessage({ kind: 'error', error: { code: 'SessionLost', message: 'lost', requestId: 'r', retryDisposition: 'never' } });
    await expect(pending).rejects.toMatchObject({ code: 'SessionLost' });
    await iterator.return?.();
  });
});
