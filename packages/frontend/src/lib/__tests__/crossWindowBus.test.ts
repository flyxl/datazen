import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

/**
 * Cross-window bus tests (form-split F6 prep).
 *
 * The bus rides the `EventChannel` port when a platform is bound and the
 * shared BroadcastChannel default otherwise — these two describes pin both
 * halves. The desktop (Tauri emit/listen) implementation is app-owned and is
 * covered by `src/platform/__tests__/eventChannel.desktop.test.ts`.
 */

import { clearPlatform, configurePlatform, testPlatform } from '../../test/setup';
import { emitCrossWindow, listenCrossWindow } from '../crossWindowBus';

describe('crossWindowBus — unconfigured platform (BroadcastChannel default)', () => {
  beforeEach(() => {
    clearPlatform();
    vi.clearAllMocks();
  });

  afterEach(() => {
    clearPlatform();
  });

  it('emitCrossWindow posts via BroadcastChannel', async () => {
    const messages: unknown[] = [];
    const bc = new BroadcastChannel('datazen-bus');
    bc.onmessage = (e) => messages.push(e.data);

    await emitCrossWindow('test:event', { foo: 1 });
    await new Promise((r) => setTimeout(r, 10));
    expect(messages).toContainEqual({ event: 'test:event', payload: { foo: 1 } });
    bc.close();
  });

  it('listenCrossWindow receives BroadcastChannel messages', async () => {
    const handler = vi.fn();
    const unlisten = await listenCrossWindow('sync:settings', handler);

    const bc = new BroadcastChannel('datazen-bus');
    bc.postMessage({ event: 'sync:settings', payload: 'dark' });
    await new Promise((r) => setTimeout(r, 10));
    expect(handler).toHaveBeenCalledWith('dark');

    unlisten();
    bc.postMessage({ event: 'sync:settings', payload: 'light' });
    await new Promise((r) => setTimeout(r, 10));
    expect(handler).toHaveBeenCalledTimes(1);
    bc.close();
  });
});

describe('crossWindowBus — bound platform (port contract)', () => {
  const spyEmit = vi.fn().mockResolvedValue(undefined);
  const spyListen = vi.fn().mockResolvedValue(() => undefined);

  beforeEach(() => {
    vi.clearAllMocks();
    // The shared test platform (same shape the global setup binds), with the
    // events half replaced by spies — the port contract is what this suite
    // pins, not any particular platform implementation.
    configurePlatform({
      ...testPlatform,
      events: {
        emitCrossWindow: (...args: unknown[]) => spyEmit(...args),
        listenCrossWindow: (...args: unknown[]) => spyListen(...args),
      },
    });
  });

  afterEach(() => {
    clearPlatform();
  });

  it('emitCrossWindow forwards to the bound EventChannel port', async () => {
    await emitCrossWindow('datazen:theme-pack-changed', 'pack-1');
    expect(spyEmit).toHaveBeenCalledWith('datazen:theme-pack-changed', 'pack-1');
  });

  it('listenCrossWindow forwards to the bound EventChannel port and returns its unlisten', async () => {
    const handler = vi.fn();
    const sentinel = () => undefined;
    spyListen.mockResolvedValueOnce(sentinel);
    const unlisten = await listenCrossWindow('evt', handler);
    expect(spyListen).toHaveBeenCalledWith('evt', handler);
    expect(unlisten).toBe(sentinel);
  });
});
