#!/usr/bin/env node
/**
 * Standalone resolve-drivers unit tests (node:test).
 *
 * Run: node scripts/__tests__/resolve-drivers.test.mjs
 *
 * Covers preset/expander/comma parsing and drivers-registry.json snapshot keys.
 */
import { readFileSync } from 'fs';
import { dirname, resolve } from 'path';
import { fileURLToPath } from 'url';
import { describe, it, expect } from 'vitest';
import {
  resolveDrivers,
  resolveDriverIconImport,
  wantsCodegenOnly,
  collectDriverKvSlotEntries,
  KV_SLOT_NAMES,
} from '../resolve-drivers.mjs';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '../..');

const registry = {
  postgres: { source: 'path' },
  mysql: { source: 'path' },
  sqlite: { source: 'path' },
  redis: { source: 'path' },
  mongodb: { source: 'path' },
  kiwi: { source: 'git' },
  superset: { source: 'git' },
};

describe('resolveDrivers presets', () => {
  it('resolves bare basic to the four core path drivers', () => {
    expect(resolveDrivers('basic', registry)).toEqual([
      'postgres',
      'mysql',
      'sqlite',
      'redis',
    ]);
    expect(resolveDrivers(':basic', registry)).toEqual([
      'postgres',
      'mysql',
      'sqlite',
      'redis',
    ]);
  });

  it('returns empty for stub', () => {
    expect(resolveDrivers('stub', registry)).toEqual([]);
    expect(resolveDrivers('', registry)).toEqual([]);
  });

  it('keeps bare all as path-only', () => {
    expect(resolveDrivers('all', registry)).toEqual([
      'postgres',
      'mysql',
      'sqlite',
      'redis',
      'mongodb',
    ]);
  });
});

describe('resolveDrivers expanders in comma lists', () => {
  it('expands basic / :basic then appends git drivers without duplicates', () => {
    expect(resolveDrivers('basic,superset,kiwi', registry)).toEqual([
      'postgres',
      'mysql',
      'sqlite',
      'redis',
      'superset',
      'kiwi',
    ]);
    expect(resolveDrivers(':basic,kiwi,superset', registry)).toEqual([
      'postgres',
      'mysql',
      'sqlite',
      'redis',
      'kiwi',
      'superset',
    ]);
  });

  it('expands all / :all then appends drivers without duplicates', () => {
    const expected = ['postgres', 'mysql', 'sqlite', 'redis', 'mongodb', 'superset', 'kiwi'];
    expect(resolveDrivers('all,superset,kiwi', registry)).toEqual(expected);
    expect(resolveDrivers(':all,superset,kiwi', registry)).toEqual(expected);
  });

  it('dedupes when a path driver is listed after all', () => {
    expect(resolveDrivers('all,postgres,superset', registry)).toEqual([
      'postgres',
      'mysql',
      'sqlite',
      'redis',
      'mongodb',
      'superset',
    ]);
  });

  it('accepts bare kiwi or superset as single registry ids', () => {
    expect(resolveDrivers('kiwi', registry)).toEqual(['kiwi']);
    expect(resolveDrivers('superset', registry)).toEqual(['superset']);
  });
});

describe('wantsCodegenOnly', () => {
  it('detects --codegen-only anywhere in argv', () => {
    expect(wantsCodegenOnly(['--drivers=basic'])).toBe(false);
    expect(wantsCodegenOnly(['--codegen-only'])).toBe(true);
    expect(wantsCodegenOnly(['--codegen-only', '--drivers=basic'])).toBe(true);
  });
});

describe('drivers-registry.json snapshot', () => {
  it('contains required path driver keys with source=path', () => {
    const raw = readFileSync(resolve(ROOT, 'drivers-registry.json'), 'utf-8');
    const live = JSON.parse(raw);
    for (const id of ['postgres', 'mysql', 'sqlite', 'redis']) {
      expect(live[id]).toBeTruthy();
      expect(live[id].source).toBe('path');
      expect(typeof live[id].feature).toBe('string');
    }
  });

  it('basic preset resolves only to registry path drivers that exist', () => {
    const raw = readFileSync(resolve(ROOT, 'drivers-registry.json'), 'utf-8');
    const live = JSON.parse(raw);
    const resolved = resolveDrivers('basic', live);
    expect(resolved).toEqual(['postgres', 'mysql', 'sqlite', 'redis']);
    for (const id of resolved) {
      expect(live[id]?.source).toBe('path');
    }
  });
});

describe('resolveDriverIconImport', () => {
  it('resolves badges next to the meta file', () => {
    const resolved = resolveDriverIconImport(
      '../../packages/drivers/postgres/ui/meta',
      'postgresql',
    );
    expect(resolved?.importPath).toContain('drivers/postgres/ui/icons/postgresql.svg?url');
  });

  it('falls back to the driver ui/ dir when meta sits in a nested feature dir', () => {
    const resolved = resolveDriverIconImport(
      '../../packages/drivers/redis/ui/shared/meta',
      'redis',
    );
    expect(resolved?.importPath).toContain('drivers/redis/ui/icons/redis.svg?url');
  });

  it('returns null for dbTypes without a badge SVG', () => {
    expect(
      resolveDriverIconImport('../../packages/drivers/redis/ui/shared/meta', 'no-such-db-type'),
    ).toBeNull();
  });
});

describe('collectDriverKvSlotEntries', () => {
  it('keeps the generator slot list in sync with the frozen KvSlotName contract', () => {
    const src = readFileSync(
      resolve(ROOT, 'packages/driver-sdk/src/types/kv-slots.ts'),
      'utf8',
    );
    const match = /export type KvSlotName = ([^;]+);/.exec(src);
    expect(match).not.toBeNull();
    const sdkSlots = match[1]
      .split('|')
      .map((s) => s.trim().replace(/'/g, ''))
      .filter(Boolean);
    expect(KV_SLOT_NAMES).toEqual(sdkSlots);
  });

  it('emits nothing for a driver that declares no kvSlots block', () => {
    const config = {
      postgres: { dbTypes: [{ id: 'postgresql' }] },
    };
    expect(collectDriverKvSlotEntries(config, ['postgres'])).toEqual([]);
  });

  it('treats every slot as optional so an unfilled path never has to exist', () => {
    const config = {
      redis: {
        dbTypes: [{ id: 'redis' }],
        kvSlots: {
          statusBar: { component: 'RedisStatusBar', path: '../drivers/redis/ui/kvSlots' },
          home: undefined,
        },
      },
    };
    expect(collectDriverKvSlotEntries(config, ['redis'])).toEqual([
      {
        dbType: 'redis',
        slot: 'statusBar',
        component: 'RedisStatusBar',
        path: '../drivers/redis/ui/kvSlots',
      },
    ]);
  });

  it('drops half-declared slots (component or path missing)', () => {
    const config = {
      redis: {
        dbTypes: [{ id: 'redis' }],
        kvSlots: {
          contextBar: { component: 'OnlyComponent' },
          keyPropsSidebar: { path: '../drivers/redis/ui/kvSlots' },
        },
      },
    };
    expect(collectDriverKvSlotEntries(config, ['redis'])).toEqual([]);
  });

  it('fans one contribution out over every dbType of the driver', () => {
    const config = {
      family: {
        dbTypes: [{ id: 'redis' }, { id: 'valkey' }],
        kvSlots: {
          contextBar: { component: 'KvContextBar', path: '../drivers/family/ui/kvSlots' },
        },
      },
    };
    expect(collectDriverKvSlotEntries(config, ['family']).map((e) => e.dbType)).toEqual([
      'redis',
      'valkey',
    ]);
  });
});
