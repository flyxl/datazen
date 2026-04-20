/**
 * Cross-window event bus.
 *
 * Tauri runtime  → uses Tauri event system (emit/listen broadcasts to all windows).
 * Browser dev    → uses BroadcastChannel API (works across tabs).
 */

type Handler = (payload?: unknown) => void;

const CHANNEL_NAME = 'datazen-bus';

function isTauri(): boolean {
  return '__TAURI_INTERNALS__' in globalThis;
}

let bc: BroadcastChannel | null = null;

function getBroadcastChannel(): BroadcastChannel {
  if (!bc) {
    bc = new BroadcastChannel(CHANNEL_NAME);
  }
  return bc;
}

export async function emitCrossWindow(event: string, payload?: unknown): Promise<void> {
  if (isTauri()) {
    const { emit } = await import('@tauri-apps/api/event');
    await emit(event, payload);
  } else {
    getBroadcastChannel().postMessage({ event, payload });
  }
}

export async function listenCrossWindow(
  event: string,
  handler: Handler,
): Promise<() => void> {
  if (isTauri()) {
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
