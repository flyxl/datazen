/**
 * Read-side hooks for the host-owned KV slot relay (contract F-2).
 *
 * The KV slots live in different React subtrees from the workbench, so the
 * per-panel `KvSlotState` atom is the only channel between them — and the atom's
 * getters plus `subscribe` are `useSyncExternalStore`-shaped for exactly this
 * consumer: each slot subscribes to the single field it renders, so a selection
 * change never re-renders the whole workspace.
 *
 * The host freezes the atom object for the panel's lifetime, hence `state.subscribe`
 * is a stable identity and safe to hand straight to `useSyncExternalStore`.
 */
import { useSyncExternalStore } from 'react';
import type { KvSlotState } from '@datazen/driver-sdk';

/** Key currently selected in the workbench's key tree, `null` when none. */
export function useKvSelectedKey(state: KvSlotState): string | null {
  return useSyncExternalStore(state.subscribe, state.getSelectedKey);
}

/** Whether the panel holds an unsaved editor draft (PRD §4 I-1 dirty gate). */
export function useKvDirty(state: KvSlotState): boolean {
  return useSyncExternalStore(state.subscribe, state.getDirty);
}

/**
 * Keys currently materialised in the tree (status bar `loaded n`).
 *
 * One `useSyncExternalStore` per field, and the getter is passed **by
 * reference**: F-1's scalar rule exists precisely so these can be handed to the
 * store directly. Wrapping a getter — `useSyncExternalStore(sub, () =>
 * ({command: state.getLastWriteCommand(), ms: state.getLastWriteDurationMs()}))`
 * — builds a new snapshot object on every call and loops forever, so composite
 * reads below are composed from two separate subscriptions instead.
 */
export function useKvLoadedCount(state: KvSlotState): number {
  return useSyncExternalStore(state.subscribe, state.getLoadedCount);
}

/**
 * Last SCAN cursor the tree reported; `'0'` means it wrapped.
 *
 * Reading `'0'` **alone does not mean the scan finished** (F-1): a fresh panel
 * also reports `'0'`. Renderers must pair it with {@link useKvLoadedCount} before
 * claiming completion — see `scanStateOf` in `KvStatusBar.tsx` for the pair this
 * driver's own renderer uses.
 */
export function useKvScanCursor(state: KvSlotState): string {
  return useSyncExternalStore(state.subscribe, state.getScanCursor);
}

/** Multi-selection size. The keys themselves stay in the workbench (PRD I-8). */
export function useKvSelectionCount(state: KvSlotState): number {
  return useSyncExternalStore(state.subscribe, state.getSelectionCount);
}

/** The two halves of the last acknowledged write. */
export interface KvLastWrite {
  /** e.g. `SET app:cache:session:1`; `null` ⇒ no write recorded yet. */
  command: string | null;
  /** Server round trip in ms; `null` while unknown. */
  durationMs: number | null;
}

/**
 * Last write the server acknowledged.
 *
 * Composed from **two** scalar subscriptions rather than one wrapping getter,
 * because F-2.1's rule is that a getter returning a fresh object would make
 * `useSyncExternalStore` see a new snapshot every render. Two subscriptions to
 * two frozen scalars keep the snapshot identity stable; the object this hook
 * returns is built during render, which is fine — it is not fed back into the
 * store as a snapshot.
 */
export function useKvLastWrite(state: KvSlotState): KvLastWrite {
  const command = useSyncExternalStore(state.subscribe, state.getLastWriteCommand);
  const durationMs = useSyncExternalStore(state.subscribe, state.getLastWriteDurationMs);
  return { command, durationMs };
}
