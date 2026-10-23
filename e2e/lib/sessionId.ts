/**
 * `connect` / `connect_dedicated` answer with `{ connectionId, dbSessionId }`
 * (they used to answer with the bare session id). Specs only ever need the
 * runtime session id, so every harness copy of `invokeBackend` normalizes the
 * result here instead of at each of the ~100 call sites.
 */
export const CONNECT_COMMANDS: ReadonlySet<string> = new Set(['connect', 'connect_dedicated']);

export function dbSessionIdOf(result: unknown): string {
  if (typeof result === 'string') return result;
  if (result && typeof result === 'object') {
    const id = (result as { dbSessionId?: unknown }).dbSessionId;
    if (typeof id === 'string' && id.length > 0) return id;
  }
  throw new Error('connect did not return a database session id');
}
