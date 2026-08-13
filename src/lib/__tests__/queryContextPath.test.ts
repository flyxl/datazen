import { describe, expect, it } from 'vitest';
import {
  inferSqlRelationPath,
  namespaceRootsFrom,
  resolveQueryContextPath,
} from '../queryContextPath';

describe('inferSqlRelationPath', () => {
  it('reads a MySQL-qualified table', () => {
    expect(
      inferSqlRelationPath(
        'SELECT * FROM trading_dev.t_afi_installment_order ORDER BY id DESC LIMIT 1;',
      ),
    ).toEqual(['trading_dev', 't_afi_installment_order']);
  });

  it('reads a three-level Superset path', () => {
    expect(inferSqlRelationPath('SELECT * FROM hive.snap.orders')).toEqual([
      'hive',
      'snap',
      'orders',
    ]);
  });

  it('handles quoted identifiers', () => {
    expect(inferSqlRelationPath('SELECT * FROM `trading_dev`.`t_order`')).toEqual([
      'trading_dev',
      't_order',
    ]);
  });

  it('returns empty when there is no relation', () => {
    expect(inferSqlRelationPath('SELECT 1')).toEqual([]);
  });
});

describe('resolveQueryContextPath', () => {
  it('switches MySQL selector to the SQL database', () => {
    expect(
      resolveQueryContextPath(
        'SELECT * FROM trading_dev.t_afi_installment_order ORDER BY id DESC LIMIT 1;',
        { databases: ['information_schema', 'trading_dev'], namespaceRoots: [] },
      ),
    ).toEqual(['trading_dev']);
  });

  it('does not treat PostgreSQL schema as a database', () => {
    expect(
      resolveQueryContextPath('SELECT * FROM public.users', {
        databases: ['postgres', 'app'],
        namespaceRoots: [],
      }),
    ).toBeNull();
  });

  it('resolves cascading catalog/schema for path-hierarchy roots', () => {
    expect(
      resolveQueryContextPath('SELECT * FROM hive.snap.orders', {
        databases: [],
        namespaceRoots: ['hive', 'pg'],
      }),
    ).toEqual(['hive', 'snap']);
  });

  it('ignores unqualified tables', () => {
    expect(
      resolveQueryContextPath('SELECT * FROM users', {
        databases: ['trading_dev'],
        namespaceRoots: [],
      }),
    ).toBeNull();
  });

  it('switches as soon as the user types database.', () => {
    expect(
      resolveQueryContextPath('SELECT * FROM trading_dev.', {
        databases: ['information_schema', 'trading_dev'],
        namespaceRoots: [],
      }),
    ).toEqual(['trading_dev']);
  });

  it('switches path-hierarchy as soon as catalog. or catalog.schema.', () => {
    expect(
      resolveQueryContextPath('SELECT * FROM hive.', {
        databases: [],
        namespaceRoots: ['hive', 'pg'],
      }),
    ).toEqual(['hive']);
    expect(
      resolveQueryContextPath('SELECT * FROM hive.snap.', {
        databases: [],
        namespaceRoots: ['hive', 'pg'],
      }),
    ).toEqual(['hive', 'snap']);
  });
});

describe('namespaceRootsFrom', () => {
  it('unions aliases, tree keys, and databases', () => {
    expect(namespaceRootsFrom({ hive: {}, pg: {} }, { hive: '1' }, ['extra']).sort()).toEqual([
      'extra',
      'hive',
      'pg',
    ]);
  });
});
