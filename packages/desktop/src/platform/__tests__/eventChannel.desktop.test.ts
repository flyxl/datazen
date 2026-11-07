import { describe, expect, it, vi } from 'vitest';

/**
 * Desktop `EventChannel` tests (form-split F6 prep, moved from
 * `src/lib/__tests__/crossWindowBus.test.ts`'s Tauri describe).
 *
 * The zone channel is pure Tauri now — emit/listen through the plugin module.
 * The shared accessor halves (BroadcastChannel default, port contract) are
 * covered by the lib-side test.
 */

const mockEmit = vi.fn().mockResolvedValue(undefined);
const mockListen = vi.fn().mockResolvedValue(vi.fn());

vi.mock('@tauri-apps/api/event', () => ({
  emit: (...args: unknown[]) => mockEmit(...args),
  listen: (...args: unknown[]) => mockListen(...args),
}));

import {
  createDesktopEventChannel,
  emitCrossWindow,
  listenCrossWindow,
} from '../desktop/eventChannel';

describe('desktop eventChannel — Tauri emit/listen', () => {
  it('emitCrossWindow uses Tauri emit', async () => {
    await emitCrossWindow('datazen:theme-pack-changed', 'pack-1');
    expect(mockEmit).toHaveBeenCalledWith('datazen:theme-pack-changed', 'pack-1');
  });

  it('listenCrossWindow uses Tauri listen', async () => {
    const handler = vi.fn();
    const unlisten = vi.fn();
    mockListen.mockImplementationOnce(
      async (_event: string, cb: (e: { payload: unknown }) => void) => {
        cb({ payload: { x: 1 } });
        return unlisten;
      },
    );
    const stop = await listenCrossWindow('evt', handler);
    expect(handler).toHaveBeenCalledWith({ x: 1 });
    expect(stop).toBe(unlisten);
  });

  it('createDesktopEventChannel composes the same halves on the port shape', async () => {
    const channel = createDesktopEventChannel();
    await channel.emitCrossWindow('menu:workflow');
    expect(mockEmit).toHaveBeenCalledWith('menu:workflow', undefined);
  });
});
