/**
 * cap-bridge-BUG-001 coverage — schemaStoreBridge 新增 call signature /
 * useBoundSchemaStore 部分的专属单测。
 *
 * Paths covered:
 *  1. unbound selector call / getState throw the documented message;
 *  2. bound selector and getState forwarding;
 *  3. `useBoundSchemaStore` reactive subscription inside a React component;
 *  4. syncSchemaTables / syncSchemaNamespace / registerPathAliases forwarding
 *     (explicit session delegation without changing active state).
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
    schemas: new Map([['s-1', { pathItems: {}, databases: ['db1'], loading: false }]]),
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
    expect(() => mod.useBoundSchemaStore((s) => s.schemas.get('s-1')?.databases)).toThrow(
      NOT_BOUND_MESSAGE,
    );
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
    expect(mod.useBoundSchemaStore((s) => s.schemas.get('s-1')?.databases)).toEqual(['db1']);
    expect(mod.useBoundSchemaStore((s) => s.schemas.get('s-1')?.loading)).toBe(false);
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
      const databases = mod.useBoundConnectionSchemaField('s-1', 'databases');
      return <span data-testid="databases">{databases.join(',')}</span>;
    }

    render(<DatabaseListProbe />);
    expect(screen.getByTestId('databases').textContent).toBe('db1');

    act(() => {
      realStore.setState({
        schemas: new Map([['s-1', { pathItems: {}, databases: ['db1', 'db2'], loading: false }]]),
      });
    });
    expect(screen.getByTestId('databases').textContent).toBe('db1,db2');

    // Imperative getState() reads the same live state.
    expect(mod.useBoundSchemaStore.getState().schemas.get('s-1')?.databases).toEqual([
      'db1',
      'db2',
    ]);
  });
});

describe('schemaStoreBridge sync helpers (bound)', () => {
  it('syncSchemaTables delegates to its target session without activating it', async () => {
    const mod = await importFreshBridge();
    const state = makeSchemaState();
    const { store, setStateSpy } = makeCallableStore(state);
    mod.bindSchemaStore(store);

    mod.syncSchemaTables('db1', TABLES, 'session-9');
    expect(setStateSpy).not.toHaveBeenCalled();
    expect(state.setLoadedTables).toHaveBeenCalledWith('db1', TABLES, 'session-9');
  });

  it('syncSchemaNamespace forwards options.dbSessionId and merges namespace', async () => {
    const mod = await importFreshBridge();
    const state = makeSchemaState();
    const { store, setStateSpy } = makeCallableStore(state);
    mod.bindSchemaStore(store);

    mod.syncSchemaNamespace(['db1', 'public'], 'tables', ['users'], { dbSessionId: 's-1' });
    expect(setStateSpy).not.toHaveBeenCalled();
    expect(state.mergeNamespace).toHaveBeenCalledWith(
      ['db1', 'public'],
      'tables',
      ['users'],
      's-1',
    );
  });

  it('registerPathAliases forwards entries with dbSessionId', async () => {
    const mod = await importFreshBridge();
    const state = makeSchemaState();
    const { store, setStateSpy } = makeCallableStore(state);
    mod.bindSchemaStore(store);

    const entries = [{ name: 'My DB', id: '42' }];
    mod.registerPathAliases(entries, 's-2');
    expect(setStateSpy).not.toHaveBeenCalled();
    expect(state.registerPathAliases).toHaveBeenCalledWith(entries, 's-2');
  });

  it('getCachedPathItems returns undefined while unbound (optional chaining)', async () => {
    const mod = await importFreshBridge();
    expect(mod.getCachedPathItems('1', 's-1')).toBeUndefined();
  });

  it('subscribeSchemaPathItems ignores store updates that keep pathItems identical', async () => {
    const mod = await importFreshBridge();
    const realStore = create<SchemaStoreState>()(() => makeSchemaState());
    mod.bindSchemaStore(realStore as unknown as BoundSchemaStore);

    const listener = vi.fn();
    const stop = mod.subscribeSchemaPathItems(listener, 's-1');
    expect(listener).toHaveBeenCalledTimes(1); // initial snapshot
    listener.mockClear();

    act(() => {
      realStore.setState({
        schemas: new Map(realStore.getState().schemas).set('s-1', {
          ...realStore.getState().schemas.get('s-1')!,
          loading: true,
        }),
      }); // pathItems reference unchanged
    });
    expect(listener).not.toHaveBeenCalled();

    const items: Record<string, TableInfo[]> = { '1': TABLES };
    act(() => {
      realStore.setState({
        schemas: new Map(realStore.getState().schemas).set('s-1', {
          pathItems: items,
          databases: ['db1'],
          loading: false,
        }),
      });
    });
    expect(listener).toHaveBeenCalledWith(items);

    stop();
    act(() => {
      realStore.setState({
        schemas: new Map(realStore.getState().schemas).set('s-1', {
          pathItems: {},
          databases: ['db1'],
          loading: false,
        }),
      });
    });
    // Unsubscribed: no further notifications.
    expect(listener).toHaveBeenCalledTimes(1);
  });
  it('isolates path caches by session and clears subscribers on session removal', async () => {
    const mod = await importFreshBridge();
    const realStore = create<SchemaStoreState>()(() => makeSchemaState());
    mod.bindSchemaStore(realStore);
    const listener = vi.fn();
    const stop = mod.subscribeSchemaPathItems(listener, 's-1');
    listener.mockClear();
    realStore.setState({
      schemas: new Map(realStore.getState().schemas).set('other', {
        databases: ['remote'],
        loading: false,
        pathItems: { '1': TABLES },
      }),
    });
    expect(listener).not.toHaveBeenCalled();
    expect(mod.getCachedPathItems('1', 's-1')).toBeUndefined();
    expect(mod.getCachedPathItems('1', 'other')).toEqual(TABLES);
    realStore.setState({
      schemas: new Map(realStore.getState().schemas).set('s-1', {
        databases: ['db1'],
        loading: false,
        pathItems: { '1': TABLES },
      }),
    });
    expect(listener).toHaveBeenLastCalledWith({ '1': TABLES });
    const remaining = new Map(realStore.getState().schemas);
    remaining.delete('s-1');
    realStore.setState({ schemas: remaining });
    expect(listener).toHaveBeenLastCalledWith({});
    expect(mod.getCachedPathItems('1', 'other')).toEqual(TABLES);
    stop();
  });

  it('scoped selectors never fall back to another session and reset after removal', async () => {
    const mod = await importFreshBridge();
    const realStore = create<SchemaStoreState>()(() => makeSchemaState());
    mod.bindSchemaStore(realStore);
    function Probe() {
      const databases = mod.useBoundConnectionSchemaField('new-session', 'databases');
      return <span data-testid="scoped-databases">{databases.join(',')}</span>;
    }
    render(<Probe />);
    expect(screen.getByTestId('scoped-databases').textContent).toBe('');
    act(() =>
      realStore.setState({
        schemas: new Map(realStore.getState().schemas).set('new-session', {
          pathItems: {},
          databases: ['target'],
          loading: false,
        }),
      }),
    );
    expect(screen.getByTestId('scoped-databases').textContent).toBe('target');
    act(() =>
      realStore.setState({
        schemas: new Map([
          [
            's-1',
            {
              pathItems: {},
              databases: ['db1'],
              loading: false,
            },
          ],
        ]),
      }),
    );
    expect(screen.getByTestId('scoped-databases').textContent).toBe('');
  });
});
