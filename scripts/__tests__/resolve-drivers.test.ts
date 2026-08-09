/** @vitest-environment node */
import { describe, expect, it } from 'vitest';
import { resolveDrivers } from '../resolve-drivers.mjs';

const registry = {
  postgres: { source: 'path' },
  mysql: { source: 'path' },
  sqlite: { source: 'path' },
  redis: { source: 'path' },
  mongodb: { source: 'path' },
  kiwi: { source: 'git' },
  superset: { source: 'git' },
  olap: { source: 'git' },
} as const;

describe('resolveDrivers', () => {
  it('keeps bare all as path-only', () => {
    expect(resolveDrivers('all', registry)).toEqual([
      'postgres',
      'mysql',
      'sqlite',
      'redis',
      'mongodb',
    ]);
  });

  it('expands all / :all in a list then appends drivers without duplicates', () => {
    const expected = [
      'postgres',
      'mysql',
      'sqlite',
      'redis',
      'mongodb',
      'superset',
      'kiwi',
    ];
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
});
