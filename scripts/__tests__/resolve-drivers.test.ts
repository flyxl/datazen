/** @vitest-environment node */
import { describe, expect, it } from 'vitest';
import { mkdtempSync, readFileSync, rmSync, statSync, utimesSync, writeFileSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';
import {
  parseVariant,
  resolveDrivers,
  wantsCodegenOnly,
  writeIfChanged,
} from '../resolve-drivers.mjs';

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
