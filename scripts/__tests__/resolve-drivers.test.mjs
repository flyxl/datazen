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
import { resolveDrivers, wantsCodegenOnly } from '../resolve-drivers.mjs';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '../..');

const registry = {
  postgres: { source: 'path' },
  mysql: { source: 'path' },
  sqlite: { source: 'path' },
  redis: { source: 'path' },
  mongodb: { source: 'path' },
  kiwi: { source: 'git' },
  superset: { source: 'git' },
  olap: { source: 'git' },
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
