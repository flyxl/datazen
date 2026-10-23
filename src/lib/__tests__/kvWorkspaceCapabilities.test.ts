import { readFileSync } from 'fs';
import { dirname, resolve } from 'path';
import { fileURLToPath } from 'url';
import { describe, expect, it } from 'vitest';
import type { DatabaseTypeMeta, KvWorkspaceCapabilities } from '../databaseMeta';
import { KV_SLOT_NAMES, hasKvSlotCapability } from '../kvWorkspaceCapabilities';

/**
 * Minimal meta fixture — the capability reader must not care about any of the
 * SQL/document fields, so only the required ones are filled.
 */
function metaWithKvWorkspace(kvWorkspace?: KvWorkspaceCapabilities): DatabaseTypeMeta {
  const base = {
    label: 'Fixture KV',
    shortLabel: 'FK',
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
  } satisfies DatabaseTypeMeta;

  return kvWorkspace ? { ...base, kvWorkspace } : base;
}

const FULL: KvWorkspaceCapabilities = {
  contextBar: true,
  statusBar: true,
  keyPropsSidebar: true,
  home: true,
};

describe('kvWorkspaceCapabilities', () => {
  it('exposes exactly the four slot names the generator knows', () => {
    expect([...KV_SLOT_NAMES].sort()).toEqual(
      ['connectionHome', 'contextBar', 'keyPropsSidebar', 'statusBar'].sort(),
    );
  });

  // [tester] The generator side of the list is pinned against the same union by
  // `scripts/__tests__/resolve-drivers.test.ts`; this closes the other direction.
  // `KV_SLOT_NAMES` is typed `readonly KvSlotName[]`, so a slot added to the union
  // but forgotten here would compile fine and silently desync the host slot list
  // from the frozen contract — only a source-text pin can catch it.
  it('[tester] keeps the host slot list in sync with the frozen KvSlotName union', () => {
    const src = readFileSync(
      resolve(
        dirname(fileURLToPath(import.meta.url)),
        '../../../packages/driver-sdk/src/types/kv-slots.ts',
      ),
      'utf8',
    );
    const match = /export type KvSlotName = ([^;]+);/.exec(src);
    expect(match).not.toBeNull();
    const sdkSlots = match![1]
      .split('|')
      .map((s) => s.trim().replace(/'/g, ''))
      .filter(Boolean);
    expect([...KV_SLOT_NAMES]).toEqual(sdkSlots);
  });

  it('treats a meta without kvWorkspace as "no capability" for every slot', () => {
    // Regression guard for the track's core promise: a driver that never heard of
    // KV slots (mysql / postgresql / mongodb …) keeps the pre-track rendering path.
    const meta = metaWithKvWorkspace();
    for (const slot of KV_SLOT_NAMES) {
      expect(hasKvSlotCapability(meta, slot)).toBe(false);
    }
  });

  it('treats an empty kvWorkspace declaration as "no capability"', () => {
    const meta = metaWithKvWorkspace({});
    for (const slot of KV_SLOT_NAMES) {
      expect(hasKvSlotCapability(meta, slot)).toBe(false);
    }
  });

  it('resolves connectionHome from the `home` flag and the others by their own name', () => {
    const meta = metaWithKvWorkspace(FULL);
    expect(hasKvSlotCapability(meta, 'contextBar')).toBe(true);
    expect(hasKvSlotCapability(meta, 'statusBar')).toBe(true);
    expect(hasKvSlotCapability(meta, 'keyPropsSidebar')).toBe(true);
    expect(hasKvSlotCapability(meta, 'connectionHome')).toBe(true);
  });

  it('keeps slots independent so a driver can fill only one surface', () => {
    const meta = metaWithKvWorkspace({ keyPropsSidebar: true });
    expect(hasKvSlotCapability(meta, 'keyPropsSidebar')).toBe(true);
    expect(hasKvSlotCapability(meta, 'contextBar')).toBe(false);
    expect(hasKvSlotCapability(meta, 'statusBar')).toBe(false);
    expect(hasKvSlotCapability(meta, 'connectionHome')).toBe(false);
  });

  it('requires a literal true, not just a present key', () => {
    const meta = metaWithKvWorkspace({ contextBar: false, home: false });
    expect(hasKvSlotCapability(meta, 'contextBar')).toBe(false);
    expect(hasKvSlotCapability(meta, 'connectionHome')).toBe(false);
  });

  it('answers false for a missing meta (unknown or not-yet-registered driver)', () => {
    for (const slot of KV_SLOT_NAMES) {
      expect(hasKvSlotCapability(undefined, slot)).toBe(false);
    }
  });
});
