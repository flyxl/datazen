import { describe, expect, it } from 'vitest';
import {
  computeIsMultiDatabase,
  knownTableNames,
  parsePathHierarchyDatabaseEntry,
  resolvePreferredDatabase,
  resolveVisibleDatabases,
} from '../schemaStoreHelpers';

describe('[tester] schemaStoreHelpers', () => {
  it('parsePathHierarchyDatabaseEntry handles id:name and backend suffix', () => {
    expect(parsePathHierarchyDatabaseEntry('558:presto_afi_data (presto)')).toEqual({
      id: '558',
      name: 'presto_afi_data',
    });
    expect(parsePathHierarchyDatabaseEntry('plain')).toEqual({ id: 'plain', name: 'plain' });
    expect(parsePathHierarchyDatabaseEntry('id:label only')).toEqual({
      id: 'id',
      name: 'label only',
    });
  });

  it('knownTableNames collects namespace, table, view, and path item leaves', () => {
    const names = knownTableNames(
      { public: { users: ['id'], orders: ['id'] } },
      [{ name: 'extra', tableType: 'table', schema: 'public' }],
      [{ name: 'v_users', tableType: 'view', schema: 'public' }],
      {
        '/hive/snap': [
          { name: 'hive/snap/orders', tableType: 'table', schema: 'CATALOG' },
          { name: 'hive/snap/schema', tableType: 'table', schema: 'SCHEMA' },
        ],
      },
    );
    expect(names.has('users')).toBe(true);
    expect(names.has('extra')).toBe(true);
    expect(names.has('v_users')).toBe(true);
    expect(names.has('orders')).toBe(true);
    expect(names.has('schema')).toBe(false);
  });

  it('computeIsMultiDatabase and resolve helpers stay consistent', () => {
    expect(computeIsMultiDatabase(true, 2)).toBe(true);
    expect(resolvePreferredDatabase(['a', 'b'], 'b')).toBe('b');
    expect(resolveVisibleDatabases(['a', 'b'], 'a').lockedToConfigured).toBe(true);
  });

  it('skips system databases when no database is configured', () => {
    // MySQL `SHOW DATABASES` lists information_schema first; a cold connect must
    // not default the workspace (and its ER diagram) onto it.
    expect(resolvePreferredDatabase(['information_schema', 'mysql', 'sys', 'app', 'test'])).toBe(
      'app',
    );
    expect(resolvePreferredDatabase(['postgres', 'analytics'])).toBe('analytics');
    // A server that only exposes system databases keeps the old first-entry fallback.
    expect(resolvePreferredDatabase(['information_schema', 'mysql'])).toBe('information_schema');
    // An explicit configured database still wins, even if it is a system one.
    expect(resolvePreferredDatabase(['information_schema', 'app'], 'information_schema')).toBe(
      'information_schema',
    );
    expect(resolvePreferredDatabase([])).toBeNull();
  });
});
