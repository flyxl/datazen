import { describe, expect, it } from 'vitest';
import { buildEditorSchema } from '../buildEditorSchema';

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
});
