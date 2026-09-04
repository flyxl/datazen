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
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
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
    assert.deepEqual(resolveDrivers('basic', registry), [
      'postgres',
      'mysql',
      'sqlite',
      'redis',
    ]);
    assert.deepEqual(resolveDrivers(':basic', registry), [
      'postgres',
      'mysql',
      'sqlite',
      'redis',
    ]);
  });

  it('returns empty for stub', () => {
    assert.deepEqual(resolveDrivers('stub', registry), []);
    assert.deepEqual(resolveDrivers('', registry), []);
  });

  it('keeps bare all as path-only', () => {
    assert.deepEqual(resolveDrivers('all', registry), [
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
    assert.deepEqual(resolveDrivers('basic,superset,kiwi', registry), [
      'postgres',
      'mysql',
      'sqlite',
      'redis',
      'superset',
      'kiwi',
    ]);
    assert.deepEqual(resolveDrivers(':basic,kiwi,superset', registry), [
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
    assert.deepEqual(resolveDrivers('all,superset,kiwi', registry), expected);
    assert.deepEqual(resolveDrivers(':all,superset,kiwi', registry), expected);
  });

  it('dedupes when a path driver is listed after all', () => {
    assert.deepEqual(resolveDrivers('all,postgres,superset', registry), [
      'postgres',
      'mysql',
      'sqlite',
      'redis',
      'mongodb',
      'superset',
    ]);
  });

  it('accepts bare kiwi or superset as single registry ids', () => {
    assert.deepEqual(resolveDrivers('kiwi', registry), ['kiwi']);
    assert.deepEqual(resolveDrivers('superset', registry), ['superset']);
  });
});

describe('wantsCodegenOnly', () => {
  it('detects --codegen-only anywhere in argv', () => {
    assert.equal(wantsCodegenOnly(['--drivers=basic']), false);
    assert.equal(wantsCodegenOnly(['--codegen-only']), true);
    assert.equal(wantsCodegenOnly(['--codegen-only', '--drivers=basic']), true);
  });
});

describe('drivers-registry.json snapshot', () => {
  it('contains required path driver keys with source=path', () => {
    const raw = readFileSync(resolve(ROOT, 'drivers-registry.json'), 'utf-8');
    const live = JSON.parse(raw);
    for (const id of ['postgres', 'mysql', 'sqlite', 'redis']) {
      assert.ok(live[id], `missing registry entry: ${id}`);
      assert.equal(live[id].source, 'path', `${id} must be a path driver`);
      assert.ok(typeof live[id].feature === 'string', `${id} must declare a Cargo feature`);
    }
  });

  it('basic preset resolves only to registry path drivers that exist', () => {
    const raw = readFileSync(resolve(ROOT, 'drivers-registry.json'), 'utf-8');
    const live = JSON.parse(raw);
    const resolved = resolveDrivers('basic', live);
    assert.deepEqual(resolved, ['postgres', 'mysql', 'sqlite', 'redis']);
    for (const id of resolved) {
      assert.equal(live[id]?.source, 'path');
    }
  });
});
