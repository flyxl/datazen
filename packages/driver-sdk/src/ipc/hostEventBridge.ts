/**
 * Host event bridge (form-split F4, R8 retirement).
 *
 * Driver UI panels must not import the host runtime (`@tauri-apps/api/event`)
 * — that import is the R8 boundary violation this bridge exists to remove.
 * Pushed driver events (`redis-pubsub-message`, …) are **host broadcasts**:
 * the desktop host emits them to every window, which is exactly the rail the
 * app's `EventChannel` port owns (F2 absorbed `crossWindowBus` into it).
 *
 * The app assembly binds the implementation:
 *
 * ```ts
 * bindHostEventListener((event, handler) =>
 *   requirePlatform().events.listenCrossWindow(event, handler));
 * ```
 *
 * Unbound (unit tests, a future web form) the listener is a no-op returning
 * a no-op unlisten — a panel without events renders, it just never receives
 * any. Same bridge pattern as `bindSettingsStore` / `schemaStoreBridge`.
 */

export type HostEventPayload = unknown;
export type HostEventHandler = (payload: HostEventPayload) => void;

export type HostEventListener = (event: string, handler: HostEventHandler) => Promise<() => void>;

let bound: HostEventListener | null = null;

/** Bind the host event rail. Called by the app assembly before first paint. */
export function bindHostEventListener(listener: HostEventListener): void {
  bound = listener;
}

/** Drop the binding. Exists so tests can exercise the unbound shape. */
export function unbindHostEventListener(): void {
  bound = null;
}

/**
 * Listen to one host broadcast event. Returns the unlisten function, or a
 * no-op when no rail is bound (nothing to listen to in that form factor).
 */
export async function listenHostEvent(
  event: string,
  handler: HostEventHandler,
): Promise<() => void> {
  if (!bound) return () => {};
  return bound(event, handler);
}
