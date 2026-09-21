/**
 * cap-bridge-BUG-001 coverage — schemaStoreBridge 新增 call signature /
 * useBoundSchemaStore 部分的专属单测。
 *
 * Paths covered:
 *  1. unbound selector call / getState throw the documented message;
 *  2. bound selector and getState forwarding;
 *  3. `useBoundSchemaStore` reactive subscription inside a React component;
 *  4. syncSchemaTables / syncSchemaNamespace / registerPathAliases forwarding
 *     (setState dbSessionId branch + action delegation).
 */
import { act, cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { create } from 'zustand';
import type { TableInfo } from '../../../src/types';
import type { BoundSchemaStore, SchemaStoreState } from '../src/schemaStoreBridge';

type SchemaStoreBridgeModule = typeof import('../src/schemaStoreBridge');

const NOT_BOUND_MESSAGE = 'SchemaStore has not been bound to driver-sdk yet.';

/** Fresh module instance so the unbound branch is reachable after other suites bind. */
async function importFreshBridge(): Promise<SchemaStoreBridgeModule> {
  vi.resetModules();
  return import('../src/schemaStoreBridge');
}

const TABLES: TableInfo[] = [{ name: 'users', tableType: 'table', schema: 'public', rowCount: 10 }];

function makeSchemaState(overrides?: Partial<SchemaStoreState>): SchemaStoreState {
  return {
    pathItems: {},
    databases: ['db1'],
    loading: false,
    loadForConnection: vi.fn(async () => undefined),
    setLoadedTables: vi.fn(),
    mergeNamespace: vi.fn(),
    registerPathAliases: vi.fn(),
    cachePathItems: vi.fn(),
    ...overrides,
  };
}

/** Callable zustand-shaped fake store (selector form runs outside React render). */
function makeCallableStore(initial: SchemaStoreState) {
  let state = initial;
  const setStateSpy = vi.fn((partial: Record<string, unknown>) => {
    state = { ...state, ...partial };
  });
  const store = Object.assign(<T,>(selector: (s: SchemaStoreState) => T): T => selector(state), {
    getState: (): SchemaStoreState => state,
    setState: setStateSpy,
    subscribe: () => () => undefined,
  });
  return { store: store as unknown as BoundSchemaStore, setStateSpy };
}

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe('schemaStoreBridge call signature (unbound)', () => {
  it('useBoundSchemaStore selector call throws with the documented message', async () => {
    const mod = await importFreshBridge();
    expect(() => mod.useBoundSchemaStore((s) => s.databases)).toThrow(NOT_BOUND_MESSAGE);
  });

  it('useBoundSchemaStore.getState() throws with the documented message', async () => {
    const mod = await importFreshBridge();
    expect(() => mod.useBoundSchemaStore.getState()).toThrow(NOT_BOUND_MESSAGE);
  });
});

describe('schemaStoreBridge call signature (bound forwarding)', () => {
  it('selector call returns the requested slice', async () => {
    const mod = await importFreshBridge();
    mod.bindSchemaStore(makeCallableStore(makeSchemaState()).store);
    expect(mod.useBoundSchemaStore((s) => s.databases)).toEqual(['db1']);
    expect(mod.useBoundSchemaStore((s) => s.loading)).toBe(false);
  });

  it('getState() forwards to the bound store', async () => {
    const mod = await importFreshBridge();
    const state = makeSchemaState();
    mod.bindSchemaStore(makeCallableStore(state).store);
    expect(mod.useBoundSchemaStore.getState()).toEqual(state);
  });
});

describe('schemaStoreBridge call signature (reactive subscription)', () => {
  it('re-renders the consuming component when the bound zustand store changes', async () => {
    const mod = await importFreshBridge();
    const realStore = create<SchemaStoreState>()(() => makeSchemaState());
    mod.bindSchemaStore(realStore as unknown as BoundSchemaStore);

    function DatabaseListProbe() {
      const databases = mod.useBoundSchemaStore((s) => s.databases);
      return <span data-testid="databases">{databases.join(',')}</span>;
    }

    render(<DatabaseListProbe />);
    expect(screen.getByTestId('databases').textContent).toBe('db1');

    act(() => {
      realStore.setState({ databases: ['db1', 'db2'] });
    });
    expect(screen.getByTestId('databases').textContent).toBe('db1,db2');

    // Imperative getState() reads the same live state.
    expect(mod.useBoundSchemaStore.getState().databases).toEqual(['db1', 'db2']);
  });
});

describe('schemaStoreBridge sync helpers (bound)', () => {
  it('syncSchemaTables sets dbSessionId then delegates to setLoadedTables', async () => {
    const mod = await importFreshBridge();
    const state = makeSchemaState();
    const { store, setStateSpy } = makeCallableStore(state);
    mod.bindSchemaStore(store);

    mod.syncSchemaTables('db1', TABLES, 'session-9');
    expect(setStateSpy).toHaveBeenCalledWith({ dbSessionId: 'session-9' });
    expect(state.setLoadedTables).toHaveBeenCalledWith('db1', TABLES);
  });

  it('syncSchemaTables without dbSessionId skips setState', async () => {
    const mod = await importFreshBridge();
    const state = makeSchemaState();
    const { store, setStateSpy } = makeCallableStore(state);
    mod.bindSchemaStore(store);

    mod.syncSchemaTables('db1', TABLES);
    expect(setStateSpy).not.toHaveBeenCalled();
    expect(state.setLoadedTables).toHaveBeenCalledWith('db1', TABLES);
  });

  it('syncSchemaNamespace forwards options.dbSessionId and merges namespace', async () => {
    const mod = await importFreshBridge();
    const state = makeSchemaState();
    const { store, setStateSpy } = makeCallableStore(state);
    mod.bindSchemaStore(store);

    mod.syncSchemaNamespace(['db1', 'public'], 'tables', ['users'], { dbSessionId: 's-1' });
    expect(setStateSpy).toHaveBeenCalledWith({ dbSessionId: 's-1' });
    expect(state.mergeNamespace).toHaveBeenCalledWith(['db1', 'public'], 'tables', ['users']);
  });

  it('registerPathAliases forwards entries with dbSessionId', async () => {
    const mod = await importFreshBridge();
    const state = makeSchemaState();
    const { store, setStateSpy } = makeCallableStore(state);
    mod.bindSchemaStore(store);

    const entries = [{ name: 'My DB', id: '42' }];
    mod.registerPathAliases(entries, 's-2');
    expect(setStateSpy).toHaveBeenCalledWith({ dbSessionId: 's-2' });
    expect(state.registerPathAliases).toHaveBeenCalledWith(entries);
  });

  it('syncSchemaNamespace without options still merges namespace (no setState)', async () => {
    const mod = await importFreshBridge();
    const state = makeSchemaState();
    const { store, setStateSpy } = makeCallableStore(state);
    mod.bindSchemaStore(store);

    mod.syncSchemaNamespace(['db1'], 'branch', ['public']);
    expect(setStateSpy).not.toHaveBeenCalled();
    expect(state.mergeNamespace).toHaveBeenCalledWith(['db1'], 'branch', ['public']);
  });

  it('registerPathAliases without dbSessionId skips setState', async () => {
    const mod = await importFreshBridge();
    const state = makeSchemaState();
    const { store, setStateSpy } = makeCallableStore(state);
    mod.bindSchemaStore(store);

    const entries = [{ name: 'My DB', id: '42' }];
    mod.registerPathAliases(entries);
    expect(setStateSpy).not.toHaveBeenCalled();
    expect(state.registerPathAliases).toHaveBeenCalledWith(entries);
  });

  it('getCachedPathItems returns undefined while unbound (optional chaining)', async () => {
    const mod = await importFreshBridge();
    expect(mod.getCachedPathItems('1')).toBeUndefined();
  });

  it('subscribeSchemaPathItems ignores store updates that keep pathItems identical', async () => {
    const mod = await importFreshBridge();
    const realStore = create<SchemaStoreState>()(() => makeSchemaState());
    mod.bindSchemaStore(realStore as unknown as BoundSchemaStore);

    const listener = vi.fn();
    const stop = mod.subscribeSchemaPathItems(listener);
    expect(listener).toHaveBeenCalledTimes(1); // initial snapshot
    listener.mockClear();

    act(() => {
      realStore.setState({ loading: true }); // pathItems reference unchanged
    });
    expect(listener).not.toHaveBeenCalled();

    const items: Record<string, TableInfo[]> = { '1': TABLES };
    act(() => {
      realStore.setState({ pathItems: items });
    });
    expect(listener).toHaveBeenCalledWith(items);

    stop();
    act(() => {
      realStore.setState({ pathItems: {} });
    });
    // Unsubscribed: no further notifications.
    expect(listener).toHaveBeenCalledTimes(1);
  });
});
