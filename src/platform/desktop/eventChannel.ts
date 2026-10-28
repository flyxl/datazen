/**
 * Desktop `EventChannel` implementation (form-split F2, absorbed from
 * `src/lib/crossWindowBus.ts`).
 *
 * Tauri runtime  → Tauri event system (emit/listen broadcasts to all windows).
 * Browser dev    → BroadcastChannel API (works across tabs).
 *
 * The branch is resolved at COMPILE time via `IS_DESKTOP_BUILD` (ruling #5),
 * not by probing `__TAURI_INTERNALS__` at runtime.
 */

import { IS_DESKTOP_BUILD } from './env';
import type { EventChannel } from '../contracts';

type Handler = (payload?: unknown) => void;

const CHANNEL_NAME = 'datazen-bus';

let bc: BroadcastChannel | null = null;

function getBroadcastChannel(): BroadcastChannel {
  if (!bc) {
    bc = new BroadcastChannel(CHANNEL_NAME);
  }
  return bc;
}

export async function emitCrossWindow(event: string, payload?: unknown): Promise<void> {
  if (IS_DESKTOP_BUILD) {
    const { emit } = await import('@tauri-apps/api/event');
    await emit(event, payload);
  } else {
    getBroadcastChannel().postMessage({ event, payload });
  }
}

export async function listenCrossWindow(event: string, handler: Handler): Promise<() => void> {
  if (IS_DESKTOP_BUILD) {
    const { listen } = await import('@tauri-apps/api/event');
    const unlisten = await listen(event, (e) => handler(e.payload));
    return unlisten;
  }

  const channel = getBroadcastChannel();
  const onMessage = (e: MessageEvent) => {
    if (e.data?.event === event) handler(e.data.payload);
  };
  channel.addEventListener('message', onMessage);
  return () => channel.removeEventListener('message', onMessage);
}

export function createDesktopEventChannel(): EventChannel {
  return {
    emitCrossWindow,
    listenCrossWindow,
  };
}
