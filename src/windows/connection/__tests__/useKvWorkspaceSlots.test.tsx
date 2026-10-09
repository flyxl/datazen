import { beforeEach, describe, expect, it, vi } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import type { KvSlotName } from '@datazen/driver-sdk';
import type { DatabaseType } from '../../../types';
import type { ConnectionContext, Panel, RedisDbPanel } from '../../../stores/panelStore';
import type { DatabaseTypeMeta } from '../../../lib/databaseMeta';
import { resetKvSlotStatesForTests } from '../../../lib/kvSlotState';
import { useKvWorkspaceSlots, resolveKvDatabaseIndex } from '../useKvWorkspaceSlots';

const { contributed, registry } = vi.hoisted(() => ({
  /** `${dbType}:${slot}` → component registered by this build (codegen stand-in). */
  contributed: new Map<string, unknown>(),
  registry: {} as Record<string, unknown>,
}));

vi.mock('../../../lib/databaseTypes', () => ({ DB_REGISTRY: registry }));
vi.mock('../../../extensions/generated', () => ({
  getDriverKvSlot: (dbType: string, slot: KvSlotName) => contributed.get(`${dbType}:${slot}`),
}));

const ContextBarFixture = () => null;
const StatusBarFixture = () => null;
const KeyPropsFixture = () => null;
const HomeFixture = () => null;

function registerMeta(dbType: string, over: Partial<DatabaseTypeMeta> = {}) {
  registry[dbType] = {
    label: dbType,
    shortLabel: 'FX',
    iconBg: 'bg-red-600',
    iconColor: 'text-red-400',
    defaultPort: 0,
    defaultHost: '127.0.0.1',
    defaultUser: '',
    quoteChar: '`',
    connectionMode: 'server',
    supportsSSH: false,
    supportsSSL: false,
    supportsBackup: false,
    supportsTables: false,
    isKeyValue: true,
    supportsSQL: false,
    category: 'kv',
    connectionView: 'keyvalue',
    databaseFieldType: 'index',
    connectionForm: 'standard',
    ...over,
  } satisfies DatabaseTypeMeta;
}

function contributeAll(dbType: string) {
  contributed.set(`${dbType}:contextBar`, ContextBarFixture);
  contributed.set(`${dbType}:statusBar`, StatusBarFixture);
  contributed.set(`${dbType}:keyPropsSidebar`, KeyPropsFixture);
  contributed.set(`${dbType}:connectionHome`, HomeFixture);
}

const PANEL = {
  id: 'panel-kv-1',
  type: 'redis-db',
  connectionId: 'cfg-1',
  dbSessionId: 'sess-1',
  connectionName: 'KV Local',
  databaseType: 'kvfull',
  dbName: 'db7',
} as unknown as Panel;

const HOME_CONTEXT: ConnectionContext = {
  connectionId: 'cfg-1',
  dbSessionId: 'sess-1',
  connectionName: 'KV Local',
  // Fake driver id, like the `kvfull` cast in `args()`: the registry is mocked here.
  databaseType: 'kvhome' as DatabaseType,
};

/** Module-level stand-in for the host dispatcher: one identity, like the real one. */
const ON_SLOT_ACTION = () => {};

function args(over: Partial<Parameters<typeof useKvWorkspaceSlots>[0]> = {}) {
  return {
    activePanel: PANEL,
    isKvPanel: true,
    databaseType: 'kvfull' as DatabaseType,
    database: 'db7',
    dbSessionId: 'sess-1',
    connectionId: 'cfg-1',
    connectionName: 'KV Local',
    connectionContext: null,
    initialDatabase: undefined,
    // Stable by default: the host dispatcher must keep one identity, so the
    // "props stay stable" assertions below are not disturbed by a fresh mock.
    onSlotAction: ON_SLOT_ACTION,
    ...over,
  };
}

beforeEach(() => {
  resetKvSlotStatesForTests();
  contributed.clear();
  vi.resetModules();
});

describe('useKvWorkspaceSlots', () => {
  it('binds every declared + contributed slot to the same panel props', () => {
    registerMeta('kvfull', {
      kvWorkspace: { contextBar: true, statusBar: true, keyPropsSidebar: true, home: true },
    });
    contributeAll('kvfull');
    const onSlotAction = vi.fn();

    const { result } = renderHook(() => useKvWorkspaceSlots(args({ onSlotAction })));
    const slots = result.current;

    expect(slots.contextBar?.Component).toBe(ContextBarFixture);
    expect(slots.statusBar?.Component).toBe(StatusBarFixture);
    expect(slots.keyPropsSidebar?.Component).toBe(KeyPropsFixture);
    // The landing screen is only handed over when no panel is open.
    expect(slots.connectionHome).toBeUndefined();

    // The shared bundle: exactly `KvPanelSlotProps`. `request` is deliberately NOT
    // in it — the context bar is the only surface allowed to ask the host for
    // something, so the base object stays free of an action channel.
    expect(slots.statusBar?.props).toEqual({
      connectionId: 'cfg-1',
      dbSessionId: 'sess-1',
      connectionName: 'KV Local',
      databaseType: 'kvfull',
      database: 'db7',
      dbIndex: 7,
      state: slots.panelState,
    });
    expect(slots.contextBar?.props).toEqual({
      ...slots.statusBar?.props,
      request: onSlotAction,
    });
    // The frozen contract: the drawer owns `open`/`onClose`, the toolbar owns
    // `compact`, so neither leaks into the shared props bundle.
    expect(slots.keyPropsSidebar?.props).toBe(slots.statusBar?.props);
    expect(slots.keyPropsSidebar?.props).not.toHaveProperty('open');
    expect(slots.keyPropsSidebar?.props).not.toHaveProperty('onClose');
    expect(slots.contextBar?.props).not.toHaveProperty('compact');
    // `request` reaches the context bar only; the other slots stay prop-identical.
    expect(slots.contextBar?.props.request).toBe(onSlotAction);
    expect(slots.statusBar?.props).not.toHaveProperty('request');
    expect(slots.keyPropsSidebar?.props).not.toHaveProperty('request');
    // An action handed to the context bar arrives at the host dispatcher unchanged.
    act(() => {
      slots.contextBar?.props.request({ type: 'setScanBudget', value: 5_000 });
    });
    expect(onSlotAction).toHaveBeenCalledWith({ type: 'setScanBudget', value: 5_000 });
  });

  it('hands the same state atom to the panel slots and keeps props stable across renders', () => {
    registerMeta('kvfull', { kvWorkspace: { contextBar: true, statusBar: true } });
    contributeAll('kvfull');

    const { result, rerender } = renderHook(
      ({ panel }) => useKvWorkspaceSlots(args({ activePanel: panel })),
      { initialProps: { panel: PANEL } },
    );
    const firstProps = result.current.contextBar?.props;
    const firstState = result.current.panelState;
    // Re-render with a *new* panel object (same id): driver components are
    // allowed to memo on these props, so the bundle identity must not churn.
    rerender({ panel: { ...PANEL } });

    expect(firstState).toBeDefined();
    expect(result.current.panelState).toBe(firstState);
    expect(result.current.contextBar?.props.state).toBe(firstState);
    expect(result.current.statusBar?.props.state).toBe(firstState);
    expect(result.current.contextBar?.props).toBe(firstProps);
  });

  // [tester] `request` lives in the context-bar bundle only, so a dispatcher
  // identity that churns (a caller passing an inline arrow) can at worst
  // re-render the context bar: the status bar and the sidebar keep their object.
  it('[tester] keeps the other slots untouched when the dispatcher identity churns', () => {
    registerMeta('kvfull', {
      kvWorkspace: { contextBar: true, statusBar: true, keyPropsSidebar: true },
    });
    contributeAll('kvfull');

    const { result, rerender } = renderHook(
      ({ onSlotAction }) => useKvWorkspaceSlots(args({ onSlotAction })),
      { initialProps: { onSlotAction: vi.fn() } },
    );
    const firstBar = result.current.contextBar;
    const firstStatus = result.current.statusBar;

    rerender({ onSlotAction: vi.fn() });

    expect(result.current.contextBar?.props.request).not.toBe(firstBar?.props.request);
    expect(result.current.statusBar).toBe(firstStatus);
    expect(result.current.keyPropsSidebar?.props).toBe(result.current.statusBar?.props);
  });

  it('returns no bindings when the driver declares the capability but contributed no component', () => {
    // Capability-without-implementation path: meta says "I can", this build ships
    // nothing ⇒ status quo.
    registerMeta('kvcapable-only', {
      kvWorkspace: { contextBar: true, statusBar: true, keyPropsSidebar: true, home: true },
    });

    const { result } = renderHook(() =>
      useKvWorkspaceSlots(args({ databaseType: 'kvcapable-only' as DatabaseType })),
    );

    expect(result.current.contextBar).toBeUndefined();
    expect(result.current.statusBar).toBeUndefined();
    expect(result.current.keyPropsSidebar).toBeUndefined();
    expect(result.current.connectionHome).toBeUndefined();
    // The panel state atom still exists, but nothing reads it — no crash either.
    expect(result.current.panelState).toBeDefined();
  });

  it('ignores contributed components for a driver without the capability', () => {
    registerMeta('kvundeclared');
    contributeAll('kvundeclared');

    const { result } = renderHook(() =>
      useKvWorkspaceSlots(args({ databaseType: 'kvundeclared' as DatabaseType })),
    );

    expect(result.current.contextBar).toBeUndefined();
    expect(result.current.statusBar).toBeUndefined();
    expect(result.current.keyPropsSidebar).toBeUndefined();
    expect(result.current.connectionHome).toBeUndefined();
    expect(result.current.panelState).toBeDefined();
  });

  it('opens no in-panel slot while the active panel is not a KV panel', () => {
    registerMeta('kvfull', { kvWorkspace: { contextBar: true } });
    contributeAll('kvfull');

    const { result } = renderHook(() => useKvWorkspaceSlots(args({ isKvPanel: false })));

    expect(result.current.contextBar).toBeUndefined();
    expect(result.current.statusBar).toBeUndefined();
    expect(result.current.keyPropsSidebar).toBeUndefined();
    expect(result.current.panelState).toBeUndefined();
  });

  it('yields the landing screen to the driver home only while no panel is open', () => {
    registerMeta('kvhome', { kvWorkspace: { home: true } });
    contributed.set('kvhome:connectionHome', HomeFixture);

    const { result } = renderHook(() =>
      useKvWorkspaceSlots(
        args({
          activePanel: null,
          databaseType: 'kvhome' as DatabaseType,
          connectionContext: HOME_CONTEXT,
          initialDatabase: 'db3',
        }),
      ),
    );

    expect(result.current.connectionHome?.Component).toBe(HomeFixture);
    expect(result.current.connectionHome?.props).toEqual({
      connectionId: 'cfg-1',
      dbSessionId: 'sess-1',
      connectionName: 'KV Local',
      databaseType: 'kvhome',
      initialDatabase: 'db3',
    });
    // No panel ⇒ no selection/dirty atom, no in-panel clusters.
    expect(result.current.panelState).toBeUndefined();
    expect(result.current.contextBar).toBeUndefined();

    const withPanel = renderHook(() =>
      useKvWorkspaceSlots(
        args({
          activePanel: { ...PANEL, databaseType: 'kvhome' } as unknown as Panel,
          databaseType: 'kvhome' as DatabaseType,
          connectionContext: HOME_CONTEXT,
        }),
      ),
    );
    expect(withPanel.result.current.connectionHome).toBeUndefined();
  });

  // [tester] Keep-alive tabs: two panels of the *same* connection on two
  // databases must never share a selection. Switching the active panel has to swap
  // the relay wholesale (atom + every slot props bundle), otherwise the context bar
  // of db5 keeps showing the key selected in db7.
  it('[tester] swaps to a fresh atom when the active panel changes on one connection', () => {
    registerMeta('kvfull', { kvWorkspace: { contextBar: true, statusBar: true } });
    contributeAll('kvfull');
    const panelDb5 = { ...PANEL, id: 'panel-kv-5', dbName: 'db5' } as unknown as RedisDbPanel;
    const panelDb7 = { ...PANEL, id: 'panel-kv-7', dbName: 'db7' } as unknown as RedisDbPanel;

    const { result, rerender } = renderHook(
      ({ panel }: { panel: RedisDbPanel }) =>
        useKvWorkspaceSlots(args({ activePanel: panel, database: panel.dbName ?? null })),
      { initialProps: { panel: panelDb5 } },
    );

    const db5State = result.current.panelState!;
    db5State.selectKey('session:in:db5');
    const db5Props = result.current.contextBar!.props;

    rerender({ panel: panelDb7 });

    const db7State = result.current.panelState!;
    expect(db7State).not.toBe(db5State);
    // No bleed-through: the new panel starts with nothing selected, and the
    // previous panel's selection stays parked on its own atom.
    expect(db7State.getSelectedKey()).toBeNull();
    expect(db5State.getSelectedKey()).toBe('session:in:db5');
    expect(result.current.contextBar!.props).not.toBe(db5Props);
    expect(result.current.contextBar!.props.database).toBe('db7');
    expect(result.current.contextBar!.props.dbIndex).toBe(7);
    expect(result.current.contextBar!.props.state).toBe(db7State);

    // Coming back reuses the parked atom (keep-alive tab, not a fresh panel).
    rerender({ panel: panelDb5 });
    expect(result.current.panelState).toBe(db5State);
    expect(result.current.panelState!.getSelectedKey()).toBe('session:in:db5');
  });

  it('resolves dbIndex from index-addressed databases only', () => {
    const indexMeta = { databaseFieldType: 'index' } as DatabaseTypeMeta;
    const nameMeta = { databaseFieldType: 'name' } as DatabaseTypeMeta;

    expect(resolveKvDatabaseIndex(indexMeta, 'db7')).toBe(7);
    expect(resolveKvDatabaseIndex(indexMeta, 'db15')).toBe(15);
    // No trailing digits / no target yet ⇒ nothing to hand the driver.
    expect(resolveKvDatabaseIndex(indexMeta, null)).toBeUndefined();
    expect(resolveKvDatabaseIndex(indexMeta, 'keyspace')).toBeUndefined();
    // A name-addressed driver is not indexed, even if the name looks numeric.
    expect(resolveKvDatabaseIndex(nameMeta, 'db7')).toBeUndefined();
    expect(resolveKvDatabaseIndex(undefined, 'db7')).toBeUndefined();
    // [tester] A pathological label must not reach the driver as a rounded float.
    expect(resolveKvDatabaseIndex(indexMeta, 'db99999999999999999999')).toBeUndefined();
  });

  // [tester] Panels are created before their `databaseType` is known in some
  // flows; the hook must then hand over the atom but no bindings, so a
  // driver component never renders against a half-resolved panel.
  it('[tester] opens no in-panel binding while the active panel has no database type', () => {
    registerMeta('kvfull', { kvWorkspace: { contextBar: true, statusBar: true } });
    contributeAll('kvfull');

    const { result } = renderHook(() => useKvWorkspaceSlots(args({ databaseType: undefined })));

    expect(result.current.panelState).toBeDefined();
    expect(result.current.contextBar).toBeUndefined();
    expect(result.current.statusBar).toBeUndefined();
    expect(result.current.keyPropsSidebar).toBeUndefined();
  });

  // [tester] `panelId` is the atom key; a panel without one must not mint an atom
  // under `undefined` that every such panel would then share.
  it('[tester] creates no state atom for a panel without an id', () => {
    registerMeta('kvfull', { kvWorkspace: { contextBar: true } });
    contributeAll('kvfull');

    const { result } = renderHook(() =>
      useKvWorkspaceSlots(args({ activePanel: { ...PANEL, id: '' } as unknown as Panel })),
    );

    expect(result.current.panelState).toBeUndefined();
    expect(result.current.contextBar).toBeUndefined();
  });

  // [tester] Degradation state 2 on the landing screen (the generator-side twin of
  // `kvWorkspaceSlots.test.ts`): capability declared, nothing contributed ⇒ the
  // host keeps its own banner page and must not throw.
  it('[tester] keeps the host landing screen when the home slot is capable but uncontributed', () => {
    registerMeta('kvhome-capable-only', { kvWorkspace: { home: true } });

    const { result } = renderHook(() =>
      useKvWorkspaceSlots(
        args({
          activePanel: null,
          databaseType: 'kvhome-capable-only' as DatabaseType,
          connectionContext: {
            ...HOME_CONTEXT,
            databaseType: 'kvhome-capable-only' as DatabaseType,
          },
        }),
      ),
    );

    expect(result.current.connectionHome).toBeUndefined();
    expect(result.current.panelState).toBeUndefined();
  });
});
