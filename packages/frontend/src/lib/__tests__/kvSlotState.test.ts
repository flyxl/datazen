import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { KvSlotState } from '@datazen/driver-sdk';
import {
  createKvSlotState,
  getKvSlotState,
  pruneKvSlotStates,
  resetKvSlotStatesForTests,
} from '../kvSlotState';

beforeEach(() => {
  resetKvSlotStatesForTests();
});

describe('createKvSlotState', () => {
  it('starts with no selection and no unsaved edits', () => {
    const state = createKvSlotState();
    expect(state.getSelectedKey()).toBeNull();
    expect(state.getDirty()).toBe(false);
  });

  it('notifies subscribers when the selected key changes', () => {
    const state = createKvSlotState();
    const listener = vi.fn();
    state.subscribe(listener);

    state.selectKey('user:42');

    expect(state.getSelectedKey()).toBe('user:42');
    expect(listener).toHaveBeenCalledTimes(1);
  });

  it('stays silent when a selection is replayed unchanged', () => {
    const state = createKvSlotState();
    state.selectKey('user:42');
    const listener = vi.fn();
    state.subscribe(listener);

    state.selectKey('user:42');

    expect(listener).not.toHaveBeenCalled();
  });

  it('notifies on dirty transitions in both directions only when they change', () => {
    const state = createKvSlotState();
    const listener = vi.fn();
    state.subscribe(listener);

    state.setDirty(true);
    state.setDirty(true);
    expect(state.getDirty()).toBe(true);
    expect(listener).toHaveBeenCalledTimes(1);

    state.setDirty(false);
    expect(state.getDirty()).toBe(false);
    expect(listener).toHaveBeenCalledTimes(2);
  });

  it('stops notifying after unsubscribe', () => {
    const state = createKvSlotState();
    const listener = vi.fn();
    const unsubscribe = state.subscribe(listener);

    unsubscribe();
    state.selectKey('k1');
    state.setDirty(true);

    expect(listener).not.toHaveBeenCalled();
  });

  it('notifies every subscriber, e.g. context bar and key-props sidebar at once', () => {
    const state = createKvSlotState();
    const first = vi.fn();
    const second = vi.fn();
    state.subscribe(first);
    state.subscribe(second);

    state.selectKey('k9');

    expect(first).toHaveBeenCalledTimes(1);
    expect(second).toHaveBeenCalledTimes(1);
  });
});

/**
 * Widening: one table, every new field.
 *
 * Each row pins the two properties the contract's *shape* exists to guarantee:
 * a publish that changes nothing must stay silent (otherwise a scan tick or a
 * tree scroll re-renders every slot in the panel), and the getter must hand
 * back a stable scalar (otherwise `useSyncExternalStore` sees a new snapshot on
 * every call and re-renders forever). Written once per field instead of copy-
 * pasted per field, so adding a field cannot silently skip one of the two.
 */
const WIDENED_FIELDS: Array<{
  /** Human label for the case names (field name, never UI copy). */
  name: string;
  /** Value the relay starts at. */
  initial: string | number | boolean | null;
  /** A different value of the same kind. */
  next: string | number | boolean | null;
  read: (state: KvSlotState) => string | number | boolean | null;
  write: (state: KvSlotState, value: string | number | boolean | null) => void;
}> = [
  {
    name: 'loadedCount',
    initial: 0,
    next: 52,
    read: (s) => s.getLoadedCount(),
    write: (s, v) => s.setLoadedCount(v as number),
  },
  {
    name: 'scanCursor',
    initial: '0',
    next: '1732-0',
    read: (s) => s.getScanCursor(),
    write: (s, v) => s.setScanCursor(v as string),
  },
  {
    name: 'scanning',
    initial: false,
    next: true,
    read: (s) => s.isScanning(),
    write: (s, v) => s.setScanning(v as boolean),
  },
  {
    name: 'scanBudgetUsed',
    initial: 0,
    next: 12000,
    read: (s) => s.getScanBudgetUsed(),
    write: (s, v) => s.setScanBudget(v as number, 50000),
  },
  {
    name: 'scanBudgetTotal',
    initial: 0,
    next: 200000,
    read: (s) => s.getScanBudgetTotal(),
    write: (s, v) => s.setScanBudget(12000, v as number),
  },
  {
    name: 'selectionCount',
    initial: 0,
    next: 3,
    read: (s) => s.getSelectionCount(),
    write: (s, v) => s.setSelectionCount(v as number),
  },
  {
    name: 'lastWriteCommand',
    initial: null,
    next: 'SET app:cache:session:1',
    read: (s) => s.getLastWriteCommand(),
    write: (s, v) => s.recordWrite(v as string, 12),
  },
  {
    name: 'lastWriteDurationMs',
    initial: null,
    next: 37,
    read: (s) => s.getLastWriteDurationMs(),
    write: (s, v) => s.recordWrite('SET k', v as number),
  },
];

describe('widened KvSlotState fields', () => {
  for (const field of WIDENED_FIELDS) {
    it(`${field.name}: starts at the documented default`, () => {
      expect(field.read(createKvSlotState())).toEqual(field.initial);
    });

    it(`${field.name}: publishes a new value and notifies once`, () => {
      const state = createKvSlotState();
      const listener = vi.fn();
      state.subscribe(listener);

      field.write(state, field.next);

      expect(field.read(state)).toEqual(field.next);
      expect(listener).toHaveBeenCalledTimes(1);
    });

    it(`${field.name}: replaying the current value notifies nobody`, () => {
      const state = createKvSlotState();
      field.write(state, field.next);
      const listener = vi.fn();
      state.subscribe(listener);

      field.write(state, field.next);
      field.write(state, field.next);

      expect(field.read(state)).toEqual(field.next);
      expect(listener).not.toHaveBeenCalled();
    });

    it(`${field.name}: getter is a stable scalar across repeated calls`, () => {
      const state = createKvSlotState();
      field.write(state, field.next);

      const first = field.read(state);
      // `useSyncExternalStore` compares with Object.is: a getter that built a
      // fresh value per call would fail this and loop the consumer forever.
      expect(Object.is(field.read(state), first)).toBe(true);
      expect(['string', 'number', 'boolean']).toContain(typeof first);
    });
  }

  it('returns every widened getter to a clean atom once its panel is pruned', () => {
    // The status bar reads six of these fields; a recycled panel id must not
    // inherit another panel's scan cursor or write history.
    const closed = getKvSlotState('panel-x');
    closed.setLoadedCount(9);
    closed.setScanCursor('44-0');
    closed.setScanning(true);
    closed.setScanBudget(10, 20);
    closed.setSelectionCount(4);
    closed.recordWrite('DEL gone', 5);

    pruneKvSlotStates(new Set(['panel-other']));

    const reopened = getKvSlotState('panel-x');
    expect(mergedFacts(reopened)).toEqual(mergedFacts(createKvSlotState()));
  });

  it('setScanBudget and recordWrite each publish a pair with a single notification', () => {
    const state = createKvSlotState();
    const listener = vi.fn();
    state.subscribe(listener);

    state.setScanBudget(1000, 50000);
    state.recordWrite('SET a 1', 3);

    expect(listener).toHaveBeenCalledTimes(2);
    expect(state.getScanBudgetUsed()).toBe(1000);
    expect(state.getScanBudgetTotal()).toBe(50000);
    expect(state.getLastWriteCommand()).toBe('SET a 1');
    expect(state.getLastWriteDurationMs()).toBe(3);
  });

  it('keeps the write pair consistent when only the duration moved', () => {
    // Two writes of the same command must still show the new timing — the command
    // half being unchanged may not swallow the notification.
    const state = createKvSlotState();
    state.recordWrite('SET a 1', 3);
    const listener = vi.fn();
    state.subscribe(listener);

    state.recordWrite('SET a 1', 9);

    expect(state.getLastWriteDurationMs()).toBe(9);
    expect(listener).toHaveBeenCalledTimes(1);
  });

  it('leaves unrelated fields alone: a loaded-count tick does not touch the selection', () => {
    const state = createKvSlotState();
    state.selectKey('user:42');
    state.setDirty(true);
    const listener = vi.fn();
    state.subscribe(listener);

    state.setLoadedCount(7);

    expect([state.getSelectedKey(), state.getDirty()]).toEqual(['user:42', true]);
    expect(listener).toHaveBeenCalledTimes(1);
  });
});

/** Every scalar the relay carries, as one comparable record (test-local, no prod API). */
function mergedFacts(state: KvSlotState): Record<string, unknown> {
  return {
    selectedKey: state.getSelectedKey(),
    dirty: state.getDirty(),
    loadedCount: state.getLoadedCount(),
    scanCursor: state.getScanCursor(),
    scanning: state.isScanning(),
    scanBudgetUsed: state.getScanBudgetUsed(),
    scanBudgetTotal: state.getScanBudgetTotal(),
    selectionCount: state.getSelectionCount(),
    lastWriteCommand: state.getLastWriteCommand(),
    lastWriteDurationMs: state.getLastWriteDurationMs(),
  };
}

describe('panel-scoped KV slot atoms', () => {
  it('hands out the same atom for the same panel', () => {
    expect(getKvSlotState('panel-a')).toBe(getKvSlotState('panel-a'));
  });

  it('keeps two panels of one connection independent', () => {
    // Two KV tabs of the same Redis connection on different databases must not
    // share a selected key — hence the atoms are keyed by panel, not by session.
    const panelA = getKvSlotState('panel-a');
    const panelB = getKvSlotState('panel-b');

    panelA.selectKey('only-in-a');
    panelA.setDirty(true);

    expect(panelB.getSelectedKey()).toBeNull();
    expect(panelB.getDirty()).toBe(false);
  });

  it('prunes every atom whose panel is gone and keeps the live ones', () => {
    const closed = getKvSlotState('panel-closed');
    const live = getKvSlotState('panel-live');
    closed.selectKey('x');
    closed.setDirty(true);
    live.selectKey('y');

    pruneKvSlotStates(new Set(['panel-live']));

    expect(getKvSlotState('panel-closed')).not.toBe(closed);
    expect(getKvSlotState('panel-live')).toBe(live);
    expect(getKvSlotState('panel-live').getSelectedKey()).toBe('y');

    // [tester] The single-panel `disposeKvSlotState` case is gone, and it was
    // the only place asserting that a recycled panel id comes back *clean*. Since
    // `pruneKvSlotStates` is now the only recycling path, it must carry that
    // guarantee itself: reopening a closed panel id must not inherit the old
    // selection or the old dirty flag.
    const reopened = getKvSlotState('panel-closed');
    expect(reopened.getSelectedKey()).toBeNull();
    expect(reopened.getDirty()).toBe(false);
  });

  it('prunes nothing when every panel is still live', () => {
    const live = getKvSlotState('panel-live');
    pruneKvSlotStates(new Set(['panel-live', 'panel-other']));
    expect(getKvSlotState('panel-live')).toBe(live);
  });
});

// [tester] The notify loop iterates a *copy* of the listener set (relay
// hygiene). None of the 46 contract cases pin what that copy exists for, so these
// two cover the mutation-during-notify edges a live workspace produces: a slot
// unsubscribing while reacting to a change (panel switch unmounting the context
// bar) and a slot subscribing in the same tick (second consumer mounting).
describe('[tester] notification snapshot safety', () => {
  it('still notifies the remaining listeners when one unsubscribes mid-notify', () => {
    const state = createKvSlotState();
    const remaining = vi.fn();
    let unsubscribeFirst: () => void = () => {};
    const first = vi.fn(() => unsubscribeFirst());
    unsubscribeFirst = state.subscribe(first);
    state.subscribe(remaining);

    state.selectKey('k1');

    expect(first).toHaveBeenCalledTimes(1);
    // The snapshot still held `remaining`: removal mid-loop must not swallow it.
    expect(remaining).toHaveBeenCalledTimes(1);

    // The removal took effect for every later change — no ghost notifications.
    state.setDirty(true);
    expect(first).toHaveBeenCalledTimes(1);
    expect(remaining).toHaveBeenCalledTimes(2);
  });

  it('leaves a listener that subscribes mid-notify for the following change', () => {
    const state = createKvSlotState();
    const late = vi.fn();
    const first = vi.fn(() => void state.subscribe(late));
    state.subscribe(first);

    state.selectKey('k1');
    expect(first).toHaveBeenCalledTimes(1);
    // Not in the snapshot this round: a consumer may not observe a change that
    // predates its own subscription.
    expect(late).not.toHaveBeenCalled();

    // But it is live from the next change on.
    state.setDirty(true);
    expect(late).toHaveBeenCalledTimes(1);
  });
});
