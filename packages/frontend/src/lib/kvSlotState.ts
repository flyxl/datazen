/**
 * Host-owned per-panel KV slot state.
 *
 * A KV panel's surfaces (workbench tab, 48px context bar, status bar, key-props
 * sidebar) are mounted in three different React subtrees, yet they have to agree
 * on a handful of facts: which key is selected, whether there are unsaved edits,
 * how far the scan got, how many keys are selected, what the last write cost.
 * Neither the driver nor the host can own this alone — the driver may not import
 * host stores (a package-boundary rule) and the host must not know what a "draft" is.
 * So the host keeps an opaque, per-panel state atom and hands the same object to
 * every party through props; the contract lives in
 * `@datazen/driver-sdk` (`KvSlotState`).
 *
 * The getters + `subscribe` pair is `useSyncExternalStore`-shaped on purpose:
 * consumers subscribe to the single field they render, so key-tree selection
 * never re-renders the whole workspace.
 *
 * Two rules follow from that shape and are the reason this file is written the
 * way it is:
 *
 * 1. **The atom holds one immutable snapshot; every getter reads a scalar out of
 *    it.** A getter that returned an array or built an object would hand
 *    `useSyncExternalStore` a fresh snapshot on every call and the consumer
 *    would re-render forever ("getters are scalars" is a ruling).
 * 2. **Writing the value that is already there notifies nobody.** The tree
 *    reports scan progress on every tick; a relay that announced no-ops would
 *    cascade a re-render of every slot in the panel (and a scroll would become a
 *    render storm).
 *
 * The state is *published* by the driver and *read* by the slots — the host never
 * invents a fact and never caches one in front of this relay: state belongs to
 * the UI segment that really owns it.
 */
import type { KvSlotState } from '@datazen/driver-sdk';

/** Every field the relay carries, in one immutable record. */
interface KvSlotSnapshot {
  selectedKey: string | null;
  dirty: boolean;
  loadedCount: number;
  scanCursor: string;
  scanning: boolean;
  scanBudgetUsed: number;
  scanBudgetTotal: number;
  selectionCount: number;
  lastWriteCommand: string | null;
  lastWriteDurationMs: number | null;
}

/**
 * A freshly opened panel: nothing loaded, nothing selected, scan `'0'` reads as
 * "no scan has run" rather than "the scan finished", which is why the status bar
 * pairs the cursor with {@link KvSlotSnapshot.loadedCount}.
 */
const EMPTY_SNAPSHOT: KvSlotSnapshot = {
  selectedKey: null,
  dirty: false,
  loadedCount: 0,
  scanCursor: '0',
  scanning: false,
  scanBudgetUsed: 0,
  scanBudgetTotal: 0,
  selectionCount: 0,
  lastWriteCommand: null,
  lastWriteDurationMs: null,
};

/** Create a detached KV state atom (exported for tests and host-side callers). */
export function createKvSlotState(): KvSlotState {
  const listeners = new Set<() => void>();
  let snapshot: KvSlotSnapshot = EMPTY_SNAPSHOT;

  const notify = () => {
    // Copy first: a listener may unsubscribe (or subscribe) while being notified.
    for (const listener of [...listeners]) listener();
  };

  /**
   * Apply {@link patch} unless every field in it already holds those values.
   *
   * `Object.is` per field, so `-0`/`NaN` and object identity behave the same way
   * `useSyncExternalStore` compares snapshots, and a no-op write is silent.
   */
  const commit = (patch: Partial<KvSlotSnapshot>) => {
    for (const field of Object.keys(patch) as (keyof KvSlotSnapshot)[]) {
      if (!Object.is(snapshot[field], patch[field])) {
        snapshot = { ...snapshot, ...patch };
        notify();
        return;
      }
    }
  };

  return {
    subscribe(listener) {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    getSelectedKey() {
      return snapshot.selectedKey;
    },
    selectKey(key) {
      commit({ selectedKey: key });
    },
    getDirty() {
      return snapshot.dirty;
    },
    setDirty(next) {
      commit({ dirty: next });
    },
    getLoadedCount() {
      return snapshot.loadedCount;
    },
    setLoadedCount(n) {
      commit({ loadedCount: n });
    },
    getScanCursor() {
      return snapshot.scanCursor;
    },
    setScanCursor(cursor) {
      commit({ scanCursor: cursor });
    },
    isScanning() {
      return snapshot.scanning;
    },
    setScanning(scanning) {
      commit({ scanning });
    },
    getScanBudgetUsed() {
      return snapshot.scanBudgetUsed;
    },
    getScanBudgetTotal() {
      return snapshot.scanBudgetTotal;
    },
    setScanBudget(used, total) {
      commit({ scanBudgetUsed: used, scanBudgetTotal: total });
    },
    getSelectionCount() {
      return snapshot.selectionCount;
    },
    setSelectionCount(n) {
      commit({ selectionCount: n });
    },
    getLastWriteCommand() {
      return snapshot.lastWriteCommand;
    },
    getLastWriteDurationMs() {
      return snapshot.lastWriteDurationMs;
    },
    recordWrite(command, durationMs) {
      commit({ lastWriteCommand: command, lastWriteDurationMs: durationMs });
    },
  };
}

const atoms = new Map<string, KvSlotState>();

/**
 * The atom for {@link panelId}, created on first use.
 *
 * Keyed by panel id (not `dbSessionId`) because two panels of the same KV
 * connection can be open on different databases and must not share a selection.
 */
export function getKvSlotState(panelId: string): KvSlotState {
  let atom = atoms.get(panelId);
  if (!atom) {
    atom = createKvSlotState();
    atoms.set(panelId, atom);
  }
  return atom;
}

/**
 * Drop the atoms of every panel absent from {@link livePanelIds}.
 * Called by the workspace whenever its panel list changes, mirroring how
 * table-data slices are pruned. This is the single recycling path — the host
 * never disposes one panel's atom directly, so atoms cannot accumulate.
 */
export function pruneKvSlotStates(livePanelIds: ReadonlySet<string>): void {
  for (const panelId of [...atoms.keys()]) {
    if (!livePanelIds.has(panelId)) atoms.delete(panelId);
  }
}

/** Test helper: forget every atom (production code must not call this). */
export function resetKvSlotStatesForTests(): void {
  atoms.clear();
}
