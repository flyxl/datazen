/**
 * The foreign-key navigation stack (DB-07).
 *
 * The stack is the only history the tab bar does not have, so its rules are
 * the ones the journey depends on: a jump is recorded once, "back" and
 * "forward" are inverse, a closed tab leaves the stack, and the stack never
 * grows without a bound.
 */
import { describe, expect, it, beforeEach } from 'vitest';
import { useFkNavigationStore, resetFkNavigationEntryIds } from '../fkNavigationStore';
import { FK_NAV_MAX_DEPTH, type GridViewport } from '../../lib/fkNavigation';

function entry(overrides: Partial<Parameters<ReturnType<typeof useFkNavigationStore.getState>['push']>[0]> = {}) {
  return {
    panelId: 'panel-b',
    navKey: 'key-b',
    origin: {
      sourcePanelId: 'panel-a',
      sourceTable: 'orders',
      localColumns: ['customer_id'],
      targetTable: 'customers',
      targetColumns: ['id'],
    },
    ...overrides,
  };
}

const viewport: GridViewport = { scrollTop: 420, anchorRow: 15, anchorColumn: 'customer_id' };

describe('fkNavigationStore', () => {
  beforeEach(() => {
    resetFkNavigationEntryIds();
    useFkNavigationStore.getState().reset();
  });

  it('records a jump and clears the forward stack', () => {
    const store = useFkNavigationStore.getState();
    store.push(entry());
    store.goBack(new Set(['panel-a', 'panel-b']));
    expect(useFkNavigationStore.getState().forward).toHaveLength(1);

    useFkNavigationStore.getState().push(entry({ panelId: 'panel-c', navKey: 'key-c' }));

    expect(useFkNavigationStore.getState().forward).toHaveLength(0);
    expect(useFkNavigationStore.getState().back).toHaveLength(1);
  });

  it('goes back to the tab the jump started from', () => {
    const store = useFkNavigationStore.getState();
    store.push(entry());

    const result = useFkNavigationStore
      .getState()
      .goBack(new Set(['panel-a', 'panel-b']));

    expect(result).not.toBeNull();
    // The tab to activate is the **source** of the jump.
    expect(result?.origin.sourcePanelId).toBe('panel-a');
    expect(useFkNavigationStore.getState().back).toHaveLength(0);
    expect(useFkNavigationStore.getState().forward).toHaveLength(1);
  });

  it('goes forward to the tab the jump opened', () => {
    const store = useFkNavigationStore.getState();
    store.push(entry());
    useFkNavigationStore.getState().goBack(new Set(['panel-a', 'panel-b']));

    const result = useFkNavigationStore
      .getState()
      .goForward(new Set(['panel-a', 'panel-b']));

    expect(result?.panelId).toBe('panel-b');
    expect(useFkNavigationStore.getState().back).toHaveLength(1);
    expect(useFkNavigationStore.getState().forward).toHaveLength(0);
  });

  it('is a no-op at the bottom of the stack', () => {
    const result = useFkNavigationStore.getState().goBack(new Set(['panel-a']));

    expect(result).toBeNull();
    expect(useFkNavigationStore.getState().back).toHaveLength(0);
    expect(useFkNavigationStore.getState().forward).toHaveLength(0);
  });

  it('keeps the last jump on top when walking back twice', () => {
    const store = useFkNavigationStore.getState();
    store.push(entry({ panelId: 'panel-b', navKey: 'key-b' }));
    store.push(entry({ panelId: 'panel-c', navKey: 'key-c' }));
    const panels = new Set(['panel-a', 'panel-b', 'panel-c']);

    useFkNavigationStore.getState().goBack(panels);
    useFkNavigationStore.getState().goBack(panels);

    const forward = useFkNavigationStore.getState().forward;
    // The top of the forward stack is the entry that was left last.
    expect(forward[forward.length - 1]?.panelId).toBe('panel-b');
    expect(forward).toHaveLength(2);
  });

  it('skips entries whose tab is gone', () => {
    const store = useFkNavigationStore.getState();
    store.push(entry({ panelId: 'panel-b', navKey: 'key-b' }));
    store.push(entry({ panelId: 'panel-c', navKey: 'key-c' }));

    // "panel-c" — the top of the stack — was closed: going back lands on the
    // jump before it, and the dead entry is dropped for good.
    const result = useFkNavigationStore.getState().goBack(new Set(['panel-a', 'panel-b']));

    expect(result?.panelId).toBe('panel-b');
    expect(result?.origin.sourcePanelId).toBe('panel-a');
    expect(useFkNavigationStore.getState().back).toHaveLength(0);
    expect(useFkNavigationStore.getState().forward.map((item) => item.panelId)).toEqual([
      'panel-b',
    ]);
  });

  it('drops the entries of closed tabs when it is pruned', () => {
    const store = useFkNavigationStore.getState();
    store.push(entry({ panelId: 'panel-b', navKey: 'key-b' }));
    store.push(entry({ panelId: 'panel-c', navKey: 'key-c' }));

    useFkNavigationStore.getState().prune(new Set(['panel-a', 'panel-c']));

    expect(useFkNavigationStore.getState().back.map((item) => item.panelId)).toEqual(['panel-c']);
  });

  it('rewinds the stack to one entry and clears the forward stack', () => {
    const store = useFkNavigationStore.getState();
    const first = store.push(entry({ panelId: 'panel-b', navKey: 'key-b' }));
    store.push(entry({ panelId: 'panel-c', navKey: 'key-c' }));
    useFkNavigationStore.getState().goBack(new Set(['panel-a', 'panel-b', 'panel-c']));
    expect(useFkNavigationStore.getState().forward).toHaveLength(1);

    useFkNavigationStore.getState().rewindTo(first.id);

    expect(useFkNavigationStore.getState().back).toHaveLength(1);
    expect(useFkNavigationStore.getState().forward).toHaveLength(0);
  });

  it('finds the entry of a navigation key', () => {
    const store = useFkNavigationStore.getState();
    const created = store.push(entry({ navKey: 'key-b' }));

    expect(useFkNavigationStore.getState().findEntryIdByNavKey('key-b')).toBe(created.id);
    expect(useFkNavigationStore.getState().findEntryIdByNavKey('key-x')).toBeNull();
  });

  it('drops the oldest entry and marks the stack as truncated', () => {
    const store = useFkNavigationStore.getState();
    for (let index = 0; index <= FK_NAV_MAX_DEPTH; index += 1) {
      store.push(entry({ panelId: `panel-${index}`, navKey: `key-${index}` }));
    }

    const state = useFkNavigationStore.getState();
    expect(state.back).toHaveLength(FK_NAV_MAX_DEPTH);
    expect(state.truncated).toBe(true);
    // The oldest entry is the one that was dropped.
    expect(state.back[0]?.navKey).toBe('key-1');
  });

  it('keeps only the latest viewport of a tab and consumes it once', () => {
    const store = useFkNavigationStore.getState();
    store.recordViewport('panel-a', viewport);
    store.recordViewport('panel-a', { ...viewport, scrollTop: 900 });

    const taken = useFkNavigationStore.getState().takeViewport('panel-a');
    expect(taken?.scrollTop).toBe(900);
    // Taking the snapshot consumes it.
    expect(useFkNavigationStore.getState().takeViewport('panel-a')).toBeNull();
  });

  it('has no viewport for a tab that was never left', () => {
    expect(useFkNavigationStore.getState().takeViewport('panel-zzz')).toBeNull();
  });
});
