/**
 * cap-bridge-BUG-001 coverage — connectionStoreBridge 专属单测。
 *
 * Paths covered:
 *  1. unbound selector call / getState throw the documented message;
 *  2. bound selector and getState forwarding;
 *  3. `useBoundConnectionStore` reactive subscription inside a React component.
 */
import { act, cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { create } from 'zustand';
import type {
  BoundConnectionStore,
  ConnectionBridgeItem,
  ConnectionBridgeState,
} from '../src/connectionStoreBridge';

type ConnectionStoreBridgeModule = typeof import('../src/connectionStoreBridge');

const NOT_BOUND_MESSAGE = 'ConnectionStore has not been bound to driver-sdk yet.';

/** Fresh module instance so the unbound branch is reachable after other suites bind. */
async function importFreshBridge(): Promise<ConnectionStoreBridgeModule> {
  vi.resetModules();
  return import('../src/connectionStoreBridge');
}

const CONNECTIONS: ConnectionBridgeItem[] = [
  { id: 'conn-1', options: { family: 'redis' } },
  { id: 'conn-2', options: { family: 'postgres' } },
];

function makeInitial(): ConnectionBridgeState {
  return { connections: CONNECTIONS };
}

/** Callable zustand-shaped fake store whose selector form runs outside React render. */
function makeCallableStore(initial: ConnectionBridgeState): BoundConnectionStore {
  let state = initial;
  return Object.assign(<T,>(selector: (s: ConnectionBridgeState) => T): T => selector(state), {
    getState: (): ConnectionBridgeState => state,
    setState: (partial: Partial<ConnectionBridgeState>) => {
      state = { ...state, ...partial };
    },
  }) as unknown as BoundConnectionStore;
}

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe('connectionStoreBridge (unbound)', () => {
  it('selector call throws with the documented message', async () => {
    const mod = await importFreshBridge();
    expect(() => mod.useBoundConnectionStore((s) => s.connections)).toThrow(NOT_BOUND_MESSAGE);
  });

  it('getState() throws with the documented message', async () => {
    const mod = await importFreshBridge();
    expect(() => mod.useBoundConnectionStore.getState()).toThrow(NOT_BOUND_MESSAGE);
  });
});

describe('connectionStoreBridge (bound forwarding)', () => {
  it('selector call returns the requested slice', async () => {
    const mod = await importFreshBridge();
    mod.bindConnectionStore(makeCallableStore(makeInitial()));
    const ids = mod.useBoundConnectionStore((s) => s.connections.map((c) => c.id));
    expect(ids).toEqual(['conn-1', 'conn-2']);
  });

  it('getState() forwards to the bound store', async () => {
    const mod = await importFreshBridge();
    mod.bindConnectionStore(makeCallableStore(makeInitial()));
    expect(mod.useBoundConnectionStore.getState()).toEqual({ connections: CONNECTIONS });
  });
});

describe('connectionStoreBridge (reactive subscription)', () => {
  it('re-renders the consuming component when the bound zustand store changes', async () => {
    const mod = await importFreshBridge();
    const realStore = create<ConnectionBridgeState>()(makeInitial);
    mod.bindConnectionStore(realStore as unknown as BoundConnectionStore);

    function ConnectionCountProbe() {
      const count = mod.useBoundConnectionStore((s) => s.connections.length);
      return <span data-testid="count">{String(count)}</span>;
    }

    render(<ConnectionCountProbe />);
    expect(screen.getByTestId('count').textContent).toBe('2');

    act(() => {
      realStore.setState({
        connections: [...CONNECTIONS, { id: 'conn-3', options: {} }],
      });
    });
    expect(screen.getByTestId('count').textContent).toBe('3');

    act(() => {
      realStore.setState({ connections: [] });
    });
    expect(screen.getByTestId('count').textContent).toBe('0');
  });
});
