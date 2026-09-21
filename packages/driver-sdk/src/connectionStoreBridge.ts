/**
 * Connection store bridge — driver-SDK capability injection.
 *
 * Host binds its real `connectionStore` zustand store at module load
 * (schemaStoreBridge pattern); drivers consume persisted connection metadata
 * through `useBoundConnectionStore`.
 */

/** Minimal connection config shape consumed by driver UI. */
export type ConnectionBridgeItem = {
  id: string;
  /** Opaque per-driver connection options (e.g. Redis topology/TLS). */
  options?: Record<string, unknown>;
};

export type ConnectionBridgeState = {
  connections: ConnectionBridgeItem[];
};

export type BoundConnectionStore = {
  <T>(selector: (state: ConnectionBridgeState) => T): T;
  getState(): ConnectionBridgeState;
};

let boundStore: BoundConnectionStore | null = null;

export function bindConnectionStore(store: BoundConnectionStore): void {
  boundStore = store;
}

function getStore(): BoundConnectionStore {
  if (!boundStore) {
    throw new Error('ConnectionStore has not been bound to driver-sdk yet.');
  }
  return boundStore;
}

export type UseBoundConnectionStore = {
  <T>(selector: (state: ConnectionBridgeState) => T): T;
  getState(): ConnectionBridgeState;
};

export const useBoundConnectionStore: UseBoundConnectionStore = Object.assign(
  <T>(selector: (state: ConnectionBridgeState) => T): T => getStore()(selector),
  {
    getState: (): ConnectionBridgeState => getStore().getState(),
  },
);
