/**
 * Merged file: the former `resolve-drivers.test.mjs` carried a verbatim-duplicate
 * subset of these cases plus its own unique coverage (registry snapshot, icon
 * import, kv-slot collection). It was folded in here and deleted, so each case
 * now exists exactly once. Suite-level effect: one file fewer, 7 duplicated
 * cases fewer — the next green run's totals move accordingly and that is
 * recorded in docs/development/ci-test-matrix.md (2026-10-23 补测 note).
 */
/** @vitest-environment node */
import { describe, expect, it } from 'vitest';
import { mkdtempSync, readFileSync, rmSync, statSync, utimesSync, writeFileSync } from 'fs';
import { tmpdir } from 'os';
import { dirname, join, resolve } from 'path';
import { fileURLToPath } from 'url';
import {
  parseVariant,
  resolveDrivers,
  resolveDriverIconImport,
  wantsCodegenOnly,
  writeIfChanged,
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
} as const;

describe('resolveDrivers', () => {
  it('resolves bare basic to the four core path drivers', () => {
    expect(resolveDrivers('basic', registry)).toEqual(['postgres', 'mysql', 'sqlite', 'redis']);
    expect(resolveDrivers(':basic', registry)).toEqual(['postgres', 'mysql', 'sqlite', 'redis']);
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

  it('expands basic / :basic in a list then appends git drivers without duplicates', () => {
    const expected = ['postgres', 'mysql', 'sqlite', 'redis', 'superset', 'kiwi'];
    expect(resolveDrivers('basic,superset,kiwi', registry)).toEqual(expected);
    expect(resolveDrivers(':basic,kiwi,superset', registry)).toEqual([
      'postgres',
      'mysql',
      'sqlite',
      'redis',
      'kiwi',
      'superset',
    ]);
  });

  it('expands all / :all in a list then appends drivers without duplicates', () => {
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

describe('parseVariant', () => {
  it('reads --variant=<sku>', () => {
    expect(parseVariant(['--variant=all'], {})).toBe('all');
    expect(parseVariant(['--drivers=basic', '--variant=akulaku'], {})).toBe('akulaku');
  });

  it('falls back to DATAZEN_VARIANT', () => {
    expect(parseVariant([], { DATAZEN_VARIANT: 'all' })).toBe('all');
  });

  it('lets the flag win over the environment', () => {
    expect(parseVariant(['--variant=basic'], { DATAZEN_VARIANT: 'all' })).toBe('basic');
  });

  it('defaults to custom so a plain local build never self-updates', () => {
    // `custom` has no published channel: a build that names no SKU must not be
    // offered the manifest of whichever SKU happens to be published.
    expect(parseVariant([], {})).toBe('custom');
    expect(parseVariant([], { DATAZEN_VARIANT: '' })).toBe('custom');
  });

  it('tolerates the matrix -all spelling', () => {
    expect(parseVariant(['--variant=-all'], {})).toBe('all');
    expect(parseVariant([], { DATAZEN_VARIANT: '-akulaku' })).toBe('akulaku');
  });
});

describe('buildRootCargoPatchLines', () => {
  it('patches git driver crates and unifies datazen-driver-api onto the Host path', async () => {
    const { buildRootCargoPatchLines } = await import('../resolve-drivers.mjs');
    const lines = buildRootCargoPatchLines(['postgres', 'kiwi', 'superset'], {
      postgres: { source: 'path', path: 'packages/drivers/postgres' },
      kiwi: {
        source: 'git',
        git: 'https://github.com/flyxl/datazen-driver-kiwi.git',
      },
      superset: {
        source: 'git',
        git: 'https://github.com/flyxl/datazen-driver-superset.git',
      },
    });
    const text = lines.join('\n');
    expect(text).toContain('[patch."https://github.com/flyxl/datazen-driver-kiwi.git"]');
    expect(text).toContain('datazen-plugin-kiwi = { path = "packages/drivers/kiwi" }');
    expect(text).toContain('[patch."https://github.com/flyxl/datazen-driver-superset.git"]');
    expect(text).toContain('[patch.crates-io]');
    expect(text).toContain('datazen-driver-api = { path = "packages/driver-api" }');
  });

  it('emits no patches when only path drivers are selected', async () => {
    const { buildRootCargoPatchLines } = await import('../resolve-drivers.mjs');
    expect(
      buildRootCargoPatchLines(['postgres'], {
        postgres: { source: 'path', path: 'packages/drivers/postgres' },
      }),
    ).toEqual([]);
  });
});

describe('writeIfChanged', () => {
  // Cargo fingerprints crate sources by mtime, so a byte-identical rewrite of
  // src-tauri/src/driver_init.rs forces a full host-lib recompile for nothing.
  it('writes a missing file and creates its directory', () => {
    const dir = mkdtempSync(join(tmpdir(), 'dz-wic-'));
    const target = join(dir, 'nested', 'driver_init.rs');
    expect(writeIfChanged(target, 'fn main() {}\n')).toBe(true);
    expect(readFileSync(target, 'utf-8')).toBe('fn main() {}\n');
    rmSync(dir, { recursive: true, force: true });
  });

  it('skips the write and preserves mtime when content is identical', () => {
    const dir = mkdtempSync(join(tmpdir(), 'dz-wic-'));
    const target = join(dir, 'driver_init.rs');
    writeIfChanged(target, 'same\n');
    // Pin mtime to a known past instant; a rewrite would move it forward.
    const pinned = new Date('2020-01-01T00:00:00Z');
    utimesSync(target, pinned, pinned);
    const before = statSync(target).mtimeMs;

    expect(writeIfChanged(target, 'same\n')).toBe(false);
    expect(statSync(target).mtimeMs).toBe(before);
    expect(readFileSync(target, 'utf-8')).toBe('same\n');
    rmSync(dir, { recursive: true, force: true });
  });

  it('still writes when the content genuinely differs', () => {
    const dir = mkdtempSync(join(tmpdir(), 'dz-wic-'));
    const target = join(dir, 'driver_init.rs');
    writeIfChanged(target, 'driver-postgres\n');
    expect(writeIfChanged(target, 'driver-postgres\ndriver-mysql\n')).toBe(true);
    expect(readFileSync(target, 'utf-8')).toBe('driver-postgres\ndriver-mysql\n');
    rmSync(dir, { recursive: true, force: true });
  });

  it('rewrites when the file was tampered with, so stale output self-heals', () => {
    const dir = mkdtempSync(join(tmpdir(), 'dz-wic-'));
    const target = join(dir, 'driver_init.rs');
    writeFileSync(target, 'corrupted\n');
    expect(writeIfChanged(target, 'expected\n')).toBe(true);
    expect(readFileSync(target, 'utf-8')).toBe('expected\n');
    rmSync(dir, { recursive: true, force: true });
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
      '../../../../packages/drivers/postgres/ui/meta',
      'postgresql',
    );
    expect(resolved?.importPath).toContain('drivers/postgres/ui/icons/postgresql.svg?url');
  });

  it('falls back to the driver ui/ dir when meta sits in a nested feature dir', () => {
    const resolved = resolveDriverIconImport(
      '../../../../packages/drivers/redis/ui/shared/meta',
      'redis',
    );
    expect(resolved?.importPath).toContain('drivers/redis/ui/icons/redis.svg?url');
  });

  it('returns null for dbTypes without a badge SVG', () => {
    expect(
      resolveDriverIconImport('../../../../packages/drivers/redis/ui/shared/meta', 'no-such-db-type'),
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
    const sdkSlots = match![1]
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
