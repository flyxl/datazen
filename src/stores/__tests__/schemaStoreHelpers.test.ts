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
      [{ name: 'extra', tableType: 'TABLE', schema: 'public', rowCount: null }],
      [{ name: 'v_users', tableType: 'VIEW', schema: 'public', rowCount: null }],
      {
        '/hive/snap': [
          { name: 'hive/snap/orders', tableType: 'TABLE', schema: 'CATALOG', rowCount: null },
          { name: 'hive/snap/schema', tableType: 'TABLE', schema: 'SCHEMA', rowCount: null },
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
});
