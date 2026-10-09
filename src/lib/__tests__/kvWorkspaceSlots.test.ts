import { describe, expect, it, vi } from 'vitest';
import type { KvSlotName } from '@datazen/driver-sdk';
import type { DatabaseType } from '../../types';
import type { DatabaseTypeMeta } from '../databaseMeta';
import { getKvSlotComponent } from '../kvWorkspaceSlots';

const KV_SLOTS: readonly KvSlotName[] = [
  'contextBar',
  'statusBar',
  'keyPropsSidebar',
  'connectionHome',
];

/** dbType → the component this build registered for `slot` (codegen stand-in). */
const { contributed, registry } = vi.hoisted(() => ({
  contributed: new Map<string, unknown>(),
  registry: {} as Record<string, unknown>,
}));

vi.mock('../databaseTypes', () => ({ DB_REGISTRY: registry }));

// The codegen module is the host's only door to driver components; stubbing it
// keeps this a unit test (no driver build required) while still proving the
// host never reaches past the generated lookup.
vi.mock('../../extensions/generated', () => ({
  getDriverKvSlot: (dbType: string, slot: KvSlotName) => contributed.get(`${dbType}:${slot}`),
}));

function registerMeta(dbType: string, kvWorkspace?: DatabaseTypeMeta['kvWorkspace']) {
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
    ...(kvWorkspace ? { kvWorkspace } : {}),
  } satisfies DatabaseTypeMeta;
}

function FixtureBar() {
  return null;
}

describe('getKvSlotComponent', () => {
  it('resolves a slot only when the capability is declared AND a component was contributed', () => {
    registerMeta('kvfull', { contextBar: true });
    contributed.set('kvfull:contextBar', FixtureBar);

    expect(getKvSlotComponent('kvfull' as DatabaseType, 'contextBar')).toBe(FixtureBar);
  });

  it('degrades to undefined when the capable driver contributed no component', () => {
    // Declared-but-uncontributed gap path: capability declared, component
    // missing ⇒ host keeps its own default UI and must not throw.
    registerMeta('kvcapable-only', {
      contextBar: true,
      statusBar: true,
      keyPropsSidebar: true,
      home: true,
    });

    for (const slot of KV_SLOTS) {
      expect(getKvSlotComponent('kvcapable-only' as DatabaseType, slot)).toBeUndefined();
    }
  });

  it('ignores a contributed component when the driver declares no capability', () => {
    // The capability gate runs first: a component sitting in the registry is not
    // enough, so a driver (or a stale build) cannot light up a slot the host has
    // not been told about.
    registerMeta('kvsilent');
    for (const slot of KV_SLOTS) {
      contributed.set(`kvsilent:${slot}`, FixtureBar);
    }

    for (const slot of KV_SLOTS) {
      expect(getKvSlotComponent('kvsilent' as DatabaseType, slot)).toBeUndefined();
    }
  });

  it('resolves each slot independently', () => {
    registerMeta('kvpartial', { contextBar: true, keyPropsSidebar: true });
    contributed.set('kvpartial:contextBar', FixtureBar);
    contributed.set('kvpartial:keyPropsSidebar', FixtureBar);

    expect(getKvSlotComponent('kvpartial' as DatabaseType, 'contextBar')).toBe(FixtureBar);
    expect(getKvSlotComponent('kvpartial' as DatabaseType, 'keyPropsSidebar')).toBe(FixtureBar);
    expect(getKvSlotComponent('kvpartial' as DatabaseType, 'statusBar')).toBeUndefined();
    expect(getKvSlotComponent('kvpartial' as DatabaseType, 'connectionHome')).toBeUndefined();
  });

  it('answers undefined for an undefined or unregistered database type', () => {
    expect(getKvSlotComponent(undefined, 'contextBar')).toBeUndefined();
    expect(getKvSlotComponent('nope' as DatabaseType, 'connectionHome')).toBeUndefined();
  });
});
