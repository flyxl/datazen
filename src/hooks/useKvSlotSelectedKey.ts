/**
 * Leaf-side read of a KV panel's selected key.
 *
 * The toolbar's AI button and the assistant drawer both need to know which key the
 * panel is focused on. They subscribe to that one scalar through the host-owned
 * relay instead of the workspace passing it down, so a click in the driver's key
 * tree re-renders these two leaves and nothing above them — the reason
 * `KvSlotState` getters are scalars in the first place.
 *
 * Pure read: nothing here writes the relay, and nothing caches it. The facts built
 * from it live in `src/lib/kvAiContext.ts`.
 */
import { useSyncExternalStore } from 'react';
import type { KvSlotState } from '@datazen/driver-sdk';

/** Non-KV panel: a subscription that never fires and a snapshot of "no key". */
const NO_SUBSCRIBE = () => () => {};
const NO_SELECTED_KEY = (): string | null => null;

/**
 * The selected key of the panel behind {@link state}, or `null` while no relay
 * exists (relational panel) / nothing is selected.
 */
export function useKvSlotSelectedKey(state: KvSlotState | undefined): string | null {
  return useSyncExternalStore(
    state ? state.subscribe : NO_SUBSCRIBE,
    state ? state.getSelectedKey : NO_SELECTED_KEY,
  );
}
