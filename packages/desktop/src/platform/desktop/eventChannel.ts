/**
 * Desktop `EventChannel` implementation (form-split F2, absorbed from
 * `src/lib/crossWindowBus.ts`; slimmed to the pure desktop half in the F6
 * prep — the browser BroadcastChannel rail moved back to the shared
 * `src/lib/crossWindowBus.ts`, and the assembly binds it for browser-dev
 * runs of the desktop bundle).
 *
 * Tauri runtime  → Tauri event system (emit/listen broadcasts to all windows).
 */

import type { EventChannel } from '@datazen/frontend';

type Handler = (payload?: unknown) => void;

export async function emitCrossWindow(event: string, payload?: unknown): Promise<void> {
  const { emit } = await import('@tauri-apps/api/event');
  await emit(event, payload);
}

export async function listenCrossWindow(event: string, handler: Handler): Promise<() => void> {
  const { listen } = await import('@tauri-apps/api/event');
  const unlisten = await listen(event, (e) => handler(e.payload));
  return unlisten;
}

export function createDesktopEventChannel(): EventChannel {
  return {
    emitCrossWindow,
    listenCrossWindow,
  };
}
