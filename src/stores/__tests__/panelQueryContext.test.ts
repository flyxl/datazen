import { describe, expect, it, vi, beforeEach } from 'vitest';
import { panelTargetDatabase, panelTargetSchema } from '../panelQueryContext';
import type { QueryPanel } from '../panelTypes';

const schemaStoreState = vi.hoisted(() => ({
  schemas: new Map<
    string,
    {
      currentDatabase: string | null;
      currentSchema: string | null;
      databases: string[];
      namespaceTree: Record<string, unknown>;
      pathAliases: Record<string, string>;
    }
  >(),
}));

vi.mock('../schemaStore', () => ({
  useSchemaStore: {
    getState: () => schemaStoreState,
  },
}));

const basePanel: QueryPanel = {
  id: 'panel-1',
  type: 'query',
  title: 'Q1',
  connectionId: 'cfg-1',
  dbSessionId: 'sess-1',
  connectionName: 'Test',
  databaseType: 'postgresql',
};

describe('[tester] panelQueryContext', () => {
  beforeEach(() => {
    schemaStoreState.schemas.clear();
    schemaStoreState.schemas.set('sess-1', {
      currentDatabase: 'app',
      currentSchema: 'public',
      databases: ['app'],
      namespaceTree: {},
      pathAliases: {},
    });
  });

  it('panelTargetDatabase prefers panel.database then schema currentDatabase', () => {
    expect(panelTargetDatabase({ ...basePanel, database: 'panel_db' })).toBe('panel_db');
    expect(panelTargetDatabase(basePanel)).toBe('app');
    schemaStoreState.schemas.set('sess-1', {
      currentDatabase: null,
      currentSchema: null,
      databases: [],
      namespaceTree: {},
      pathAliases: {},
    });
    expect(panelTargetDatabase(basePanel)).toBeNull();
  });

  it('panelTargetSchema prefers panel.schema then schema currentSchema', () => {
    expect(panelTargetSchema({ ...basePanel, schema: 'sales' })).toBe('sales');
    expect(panelTargetSchema(basePanel)).toBe('public');
    schemaStoreState.schemas.set('sess-1', {
      currentDatabase: 'app',
      currentSchema: null,
      databases: ['app'],
      namespaceTree: {},
      pathAliases: {},
    });
    expect(panelTargetSchema(basePanel)).toBeNull();
  });

  it('panelTargetDatabase builds path-hierarchy pin when driver supports it', async () => {
    const { DB_REGISTRY } = await import('../../lib/databaseTypes');
    if (!Object.prototype.hasOwnProperty.call(DB_REGISTRY, 'superset')) return;

    schemaStoreState.schemas.set('sess-1', {
      currentDatabase: '558:presto_afi_data',
      currentSchema: null,
      databases: ['558:presto_afi_data'],
      namespaceTree: { hive: { snap: {} } },
      pathAliases: { hive: '558' },
    });

    const panel: QueryPanel = {
      ...basePanel,
      databaseType: 'superset',
      namespacePath: ['hive', 'snap'],
    };
    expect(panelTargetDatabase(panel)).toBe('558:presto_afi_data/hive/snap');
    expect(
      panelTargetDatabase(panel, 'SELECT * FROM hive.snap.orders'),
    ).toBe('558:presto_afi_data/hive/snap');
  });
});
