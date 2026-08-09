import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const mockEmit = vi.fn().mockResolvedValue(undefined);
const mockListen = vi.fn().mockResolvedValue(vi.fn());
const mockUnlisten = vi.fn();

vi.mock('@tauri-apps/api/event', () => ({
  emit: (...args: unknown[]) => mockEmit(...args),
  listen: (...args: unknown[]) => mockListen(...args),
}));

describe('crossWindowBus — browser', () => {
  beforeEach(() => {
    delete (globalThis as Record<string, unknown>).__TAURI_INTERNALS__;
    vi.clearAllMocks();
  });

  it('emitCrossWindow posts via BroadcastChannel', async () => {
    const { emitCrossWindow } = await import('../crossWindowBus');
    const messages: unknown[] = [];
    const bc = new BroadcastChannel('datazen-bus');
    bc.onmessage = (e) => messages.push(e.data);

    await emitCrossWindow('test:event', { foo: 1 });
    await new Promise((r) => setTimeout(r, 10));
    expect(messages).toContainEqual({ event: 'test:event', payload: { foo: 1 } });
    bc.close();
  });

  it('listenCrossWindow receives BroadcastChannel messages', async () => {
    const { listenCrossWindow } = await import('../crossWindowBus');
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

describe('crossWindowBus — Tauri', () => {
  beforeEach(() => {
    (globalThis as Record<string, unknown>).__TAURI_INTERNALS__ = {};
    vi.clearAllMocks();
    mockListen.mockResolvedValue(mockUnlisten);
  });

  afterEach(() => {
    delete (globalThis as Record<string, unknown>).__TAURI_INTERNALS__;
  });

  it('emitCrossWindow uses Tauri emit', async () => {
    vi.resetModules();
    const { emitCrossWindow } = await import('../crossWindowBus');
    await emitCrossWindow('datazen:theme-pack-changed', 'pack-1');
    expect(mockEmit).toHaveBeenCalledWith('datazen:theme-pack-changed', 'pack-1');
  });

  it('listenCrossWindow uses Tauri listen', async () => {
    vi.resetModules();
    const handler = vi.fn();
    mockListen.mockImplementation(async (_event, cb) => {
      cb({ payload: { x: 1 } });
      return mockUnlisten;
    });
    const { listenCrossWindow } = await import('../crossWindowBus');
    const unlisten = await listenCrossWindow('evt', handler);
    expect(handler).toHaveBeenCalledWith({ x: 1 });
    expect(unlisten).toBe(mockUnlisten);
  });
});
