import { describe, expect, it, vi } from 'vitest';
import { createPanelCloseNotifier } from '../panelCloseNotifier';

/**
 * A stand-in for `usePanelStore` with Zustand's subscribe semantics, so the test
 * exercises the real diffing logic without dragging in the query/history/AI
 * mocks that `panelStore` itself needs. The removal *actions* live in
 * `panelStore.test.ts`; what matters here is that one subscriber turns "an id
 * vanished from `panels`" into a close notification, since every removal path
 * funnels through a single `set(...)`.
 */
/** The slice of `PanelStateLike` the notifier subscribes to, mirrored here. */
type FakePanelState = { panels: { id: string }[] };
type FakeListener = (state: FakePanelState, prev: FakePanelState) => void;

function fakeStore(initial: string[]) {
  let panels: FakePanelState['panels'] = initial.map((id) => ({ id }));
  const listeners = new Set<FakeListener>();
  return {
    subscribe: (listener: FakeListener) => {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    /** Replace the panel list, notifying subscribers exactly like `set(panels)` would. */
    set(next: string[]) {
      const prev = panels;
      panels = next.map((id) => ({ id }));
      for (const listener of [...listeners]) listener({ panels }, { panels: prev });
    },
  };
}

describe('[tester] panelCloseNotifier', () => {
  it('fires for a panel that disappeared and stays silent for the ones that stayed', () => {
    const store = fakeStore(['a', 'b', 'c']);
    const { onPanelClosed } = createPanelCloseNotifier(store);
    const onA = vi.fn();
    const onB = vi.fn();
    const onC = vi.fn();
    onPanelClosed('a', onA);
    onPanelClosed('b', onB);
    onPanelClosed('c', onC);

    store.set(['a', 'c']); // b closed

    expect(onB).toHaveBeenCalledTimes(1);
    expect(onA).not.toHaveBeenCalled();
    expect(onC).not.toHaveBeenCalled();
  });

  it('does not fire on a tab switch, where the panel is still there', () => {
    const store = fakeStore(['a', 'b']);
    const { onPanelClosed } = createPanelCloseNotifier(store);
    const onA = vi.fn();
    onPanelClosed('a', onA);

    // A switch changes which panel is active, not which panels exist.
    store.set(['a', 'b']);

    expect(onA).not.toHaveBeenCalled();
  });

  it('fires once when several panels close in a single update', () => {
    const store = fakeStore(['a', 'b', 'c', 'd']);
    const { onPanelClosed } = createPanelCloseNotifier(store);
    const onB = vi.fn();
    const onC = vi.fn();
    onPanelClosed('b', onB);
    onPanelClosed('c', onC);

    store.set(['a']); // "close others" style removal

    expect(onB).toHaveBeenCalledTimes(1);
    expect(onC).toHaveBeenCalledTimes(1);
  });

  it('stops notifying after unsubscribe', () => {
    const store = fakeStore(['a']);
    const { onPanelClosed } = createPanelCloseNotifier(store);
    const onA = vi.fn();
    const off = onPanelClosed('a', onA);

    off();
    store.set([]);

    expect(onA).not.toHaveBeenCalled();
  });

  it('fires every handler registered for a panel', () => {
    const store = fakeStore(['a']);
    const { onPanelClosed } = createPanelCloseNotifier(store);
    const first = vi.fn();
    const second = vi.fn();
    onPanelClosed('a', first);
    onPanelClosed('a', second);

    store.set([]);

    expect(first).toHaveBeenCalledTimes(1);
    expect(second).toHaveBeenCalledTimes(1);
  });

  it('a throwing handler does not stop the tab from closing or the others firing', () => {
    const store = fakeStore(['a', 'b']);
    const { onPanelClosed } = createPanelCloseNotifier(store);
    const error = vi.spyOn(console, 'error').mockImplementation(() => {});
    const onA = vi.fn(() => {
      throw new Error('driver cleanup exploded');
    });
    const onB = vi.fn();
    onPanelClosed('a', onA);
    onPanelClosed('b', onB);

    expect(() => store.set([])).not.toThrow();
    expect(onA).toHaveBeenCalledTimes(1);
    expect(onB).toHaveBeenCalledTimes(1);
    expect(error).toHaveBeenCalled();
    error.mockRestore();
  });

  it('never fires twice for the same closure', () => {
    const store = fakeStore(['a']);
    const { onPanelClosed } = createPanelCloseNotifier(store);
    const onA = vi.fn();
    onPanelClosed('a', onA);

    store.set([]);
    store.set([]);

    expect(onA).toHaveBeenCalledTimes(1);
  });
});
