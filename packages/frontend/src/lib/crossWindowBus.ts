/**
 * Cross-window event bus (form-split F6 prep).
 *
 * Shared code emits and listens through the `EventChannel` port on the
 * platform bindings; the BroadcastChannel default below is the pre-assembly
 * and unconfigured-test rail (the same browser semantics the F2 desktop
 * implementation's compile-time browser branch carried — that branch moved
 * here when the desktop zone became app-owned). The F2 re-export shim to
 * `platform/desktop/eventChannel` is gone.
 */

import { getPlatformOrNull } from '../platform/contracts';

type Handler = (payload?: unknown) => void;

const CHANNEL_NAME = 'datazen-bus';

let bc: BroadcastChannel | null = null;

function getBroadcastChannel(): BroadcastChannel {
  if (!bc) {
    bc = new BroadcastChannel(CHANNEL_NAME);
  }
  return bc;
}

/**
 * The browser-form EventChannel: BroadcastChannel wiring across tabs. Exported
 * for the web platform stub to bind as its `events` half (single source for
 * the wire shape).
 */
export function createBrowserEventChannel() {
  return {
    async emitCrossWindow(event: string, payload?: unknown): Promise<void> {
      getBroadcastChannel().postMessage({ event, payload });
    },
    async listenCrossWindow(event: string, handler: Handler): Promise<() => void> {
      const channel = getBroadcastChannel();
      const onMessage = (e: MessageEvent) => {
        if (e.data?.event === event) handler(e.data.payload);
      };
      channel.addEventListener('message', onMessage);
      return () => channel.removeEventListener('message', onMessage);
    },
  };
}

export async function emitCrossWindow(event: string, payload?: unknown): Promise<void> {
  const channel = getPlatformOrNull()?.events;
  if (channel) {
    await channel.emitCrossWindow(event, payload);
    return;
  }
  getBroadcastChannel().postMessage({ event, payload });
}

export async function listenCrossWindow(event: string, handler: Handler): Promise<() => void> {
  const channel = getPlatformOrNull()?.events;
  if (channel) {
    return channel.listenCrossWindow(event, handler);
  }
  const bus = getBroadcastChannel();
  const onMessage = (e: MessageEvent) => {
    if (e.data?.event === event) handler(e.data.payload);
  };
  bus.addEventListener('message', onMessage);
  return () => bus.removeEventListener('message', onMessage);
}
