import { describe, expect, it } from 'vitest';
import { buildEditorSchema, hoistNamespaceChild } from '../buildEditorSchema';

describe('buildEditorSchema', () => {
  it('prefers nested tree and overlays columns', () => {
    expect(
      buildEditorSchema({
        namespaceTree: { app: { users: [] } },
        tables: [],
        views: [],
        columnMap: { users: ['id'] },
      }),
    ).toEqual({ app: { users: ['id'] } });
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
