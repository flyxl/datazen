/**
 * cap-bridge-BUG-001 coverage — settingsStoreBridge专属单测。
 *
 * Paths covered:
 *  1. unbound accessors throw the documented message (fresh module per test);
 *  2. bound selector / getState / setState (object AND updater form) forwarding;
 *  3. `useBoundSettingsStore` reactive subscription inside a React component.
 */
import { act, cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { create } from 'zustand';
import type { BoundSettingsStore, SettingsBridgeState } from '../src/settingsStoreBridge';

type SettingsStoreBridgeModule = typeof import('../src/settingsStoreBridge');

const NOT_BOUND_MESSAGE = 'SettingsStore has not been bound to driver-sdk yet.';

/** Fresh module instance so the unbound branch is reachable after other suites bind. */
async function importFreshBridge(): Promise<SettingsStoreBridgeModule> {
  vi.resetModules();
  return import('../src/settingsStoreBridge');
}

function makeInitialSettings(safeMode = false): SettingsBridgeState {
  return {
    settings: { safeMode, editorFontFamily: 'JetBrains Mono', driverSettings: {} },
  };
}

/**
 * Callable zustand-shaped fake store (selector form runs outside React render),
 * with setState recorded verbatim for forwarding assertions.
 */
function makeCallableStore(initial: SettingsBridgeState) {
  let state = initial;
  const setStateSpy = vi.fn((partial: unknown) => {
    const next =
      typeof partial === 'function'
        ? (partial as (s: SettingsBridgeState) => Partial<SettingsBridgeState>)(state)
        : (partial as Partial<SettingsBridgeState>);
    state = { ...state, ...next };
  });
  const store = Object.assign(<T,>(selector: (s: SettingsBridgeState) => T): T => selector(state), {
    getState: (): SettingsBridgeState => state,
    setState: setStateSpy,
  });
  return { store: store as unknown as BoundSettingsStore, setStateSpy };
}

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe('settingsStoreBridge (unbound)', () => {
  it('selector call throws with the documented message', async () => {
    const mod = await importFreshBridge();
    expect(() => mod.useBoundSettingsStore((s) => s.settings.safeMode)).toThrow(NOT_BOUND_MESSAGE);
  });

  it('getState() throws with the documented message', async () => {
    const mod = await importFreshBridge();
    expect(() => mod.useBoundSettingsStore.getState()).toThrow(NOT_BOUND_MESSAGE);
  });

  it('setState() throws with the documented message', async () => {
    const mod = await importFreshBridge();
    expect(() => mod.useBoundSettingsStore.setState({})).toThrow(NOT_BOUND_MESSAGE);
  });
});

describe('settingsStoreBridge (bound forwarding)', () => {
  it('selector call returns the requested slice', async () => {
    const mod = await importFreshBridge();
    const { store } = makeCallableStore(makeInitialSettings(true));
    mod.bindSettingsStore(store);
    expect(mod.useBoundSettingsStore((s) => s.settings.safeMode)).toBe(true);
    expect(mod.useBoundSettingsStore((s) => s.settings.editorFontFamily)).toBe('JetBrains Mono');
  });

  it('getState() forwards to the bound store', async () => {
    const mod = await importFreshBridge();
    const initial = makeInitialSettings(false);
    const { store } = makeCallableStore(initial);
    mod.bindSettingsStore(store);
    expect(mod.useBoundSettingsStore.getState()).toEqual(initial);
  });

  it('setState forwards the object form verbatim', async () => {
    const mod = await importFreshBridge();
    const { store, setStateSpy } = makeCallableStore(makeInitialSettings(false));
    mod.bindSettingsStore(store);
    const patch = { settings: makeInitialSettings(true).settings };
    mod.useBoundSettingsStore.setState(patch);
    expect(setStateSpy).toHaveBeenCalledTimes(1);
    expect(setStateSpy.mock.calls[0]?.[0]).toBe(patch);
    expect(mod.useBoundSettingsStore.getState().settings.safeMode).toBe(true);
  });

  it('setState forwards the updater form verbatim', async () => {
    const mod = await importFreshBridge();
    const { store, setStateSpy } = makeCallableStore(makeInitialSettings(false));
    mod.bindSettingsStore(store);
    const updater = (s: SettingsBridgeState) => ({
      settings: { ...s.settings, safeMode: !s.settings.safeMode },
    });
    mod.useBoundSettingsStore.setState(updater);
    expect(setStateSpy).toHaveBeenCalledTimes(1);
    expect(setStateSpy.mock.calls[0]?.[0]).toBe(updater);
    expect(mod.useBoundSettingsStore.getState().settings.safeMode).toBe(true);
  });
});

describe('settingsStoreBridge (reactive subscription)', () => {
  it('re-renders the consuming component when the bound zustand store changes', async () => {
    const mod = await importFreshBridge();
    const realStore = create<SettingsBridgeState>()(() => makeInitialSettings(false));
    mod.bindSettingsStore(realStore as unknown as BoundSettingsStore);

    function SafeModeProbe() {
      const safeMode = mod.useBoundSettingsStore((s) => s.settings.safeMode);
      return <span data-testid="safe-mode">{String(safeMode)}</span>;
    }

    render(<SafeModeProbe />);
    expect(screen.getByTestId('safe-mode').textContent).toBe('false');

    // Updater form through the bridge → component re-renders.
    act(() => {
      mod.useBoundSettingsStore.setState((s) => ({
        settings: { ...s.settings, safeMode: true },
      }));
    });
    expect(screen.getByTestId('safe-mode').textContent).toBe('true');

    // Object form through the bridge → component re-renders back.
    act(() => {
      mod.useBoundSettingsStore.setState({
        settings: { safeMode: false, editorFontFamily: 'mono', driverSettings: {} },
      });
    });
    expect(screen.getByTestId('safe-mode').textContent).toBe('false');
  });
});
