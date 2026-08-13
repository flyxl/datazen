import { describe, expect, it } from 'vitest';
import {
  buildEditorSchema,
  hoistNamespaceChild,
  hoistNestedTableLeaves,
} from '../buildEditorSchema';

describe('buildEditorSchema', () => {
  it('prefers nested tree and overlays columns', () => {
    expect(
      buildEditorSchema({
        namespaceTree: { app: { users: [] } },
        tables: [],
        views: [],
        columnMap: { users: ['id'] },
      }),
    ).toEqual({ app: { users: ['id'] }, users: ['id'] });
  });

  it('falls back to flat tables when namespace empty', () => {
    expect(
      buildEditorSchema({
        namespaceTree: {},
        tables: [{ name: 'users', tableType: 'table', schema: null, rowCount: null }],
        views: [],
        columnMap: { users: ['id'] },
      }),
    ).toEqual({ users: ['id'] });
  });

  it('hoists current database children for unqualified completion', () => {
    expect(
      buildEditorSchema({
        namespaceTree: { app: { users: ['id'], orders: [] }, other: { t: [] } },
        tables: [],
        views: [],
        columnMap: {},
        currentDatabase: 'app',
      }),
    ).toEqual({
      app: { users: ['id'], orders: [] },
      other: { t: [] },
      users: ['id'],
      orders: [],
      t: [],
    });
  });

  it('hoists path-hierarchy table leaves and overlays columns', () => {
    expect(
      buildEditorSchema({
        namespaceTree: { presto: { hive: { snap: { orders: [] } } } },
        tables: [],
        views: [],
        columnMap: { orders: ['id', 'amt'] },
        hoistPath: ['presto', 'hive', 'snap'],
      }),
    ).toEqual({
      presto: { hive: { snap: { orders: ['id', 'amt'] } } },
      hive: { snap: { orders: ['id', 'amt'] } },
      snap: { orders: ['id', 'amt'] },
      orders: ['id', 'amt'],
    });
  });

  it('hoists postgresql schema tables and overlays columns', () => {
    expect(
      buildEditorSchema({
        namespaceTree: { public: { users: [], orders: [] }, audit: { logs: [] } },
        tables: [
          { name: 'users', tableType: 'table', schema: 'public', rowCount: null },
          { name: 'orders', tableType: 'table', schema: 'public', rowCount: null },
        ],
        views: [],
        columnMap: { users: ['id', 'email'] },
        currentDatabase: 'app',
      }),
    ).toEqual({
      public: { users: ['id', 'email'], orders: [] },
      audit: { logs: [] },
      users: ['id', 'email'],
      orders: [],
      logs: [],
    });
  });
});

describe('hoistNamespaceChild', () => {
  it('does not overwrite existing root keys', () => {
    expect(hoistNamespaceChild({ app: { users: [] }, users: ['id'] }, 'app')).toEqual({
      app: { users: [] },
      users: ['id'],
    });
  });
});

describe('hoistNestedTableLeaves', () => {
  it('copies schema table leaves to the root', () => {
    expect(hoistNestedTableLeaves({ public: { users: [] }, audit: { logs: [] } })).toEqual({
      public: { users: [] },
      audit: { logs: [] },
      users: [],
      logs: [],
    });
  });

  it('hoists table leaves at any depth without promoting catalog/schema branches', () => {
    expect(hoistNestedTableLeaves({ hive: { snap: { t: [] } } })).toEqual({
      hive: { snap: { t: [] } },
      t: [],
    });
  });
});
