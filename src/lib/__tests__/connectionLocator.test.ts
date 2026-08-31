import { describe, expect, it } from 'vitest';
import type { ConnectionSchemaState } from '../../stores/schemaStore';
import type { ConnectionConfig } from '../../types';
import {
  groupConnectionsWithRecentSections,
  PINNED_GROUP_KEY,
  rankConnections,
  RECENT_GROUP_KEY,
} from '../connectionLocator';

function connection(overrides: Partial<ConnectionConfig> = {}): ConnectionConfig {
  return {
    id: 'c1',
    name: 'Local database',
    databaseType: 'postgresql',
    host: '127.0.0.1',
    port: 5432,
    database: 'app',
    sslMode: 'prefer',
    ...overrides,
  };
}

const schemaState = (overrides: Partial<ConnectionSchemaState> = {}) =>
  ({
    currentDatabase: null,
    currentSchema: null,
    databases: [],
    databaseType: 'postgresql',
    isMultiDatabase: true,
    tables: [],
    views: [],
    schemaNames: [],
    columnMap: {},
    namespaceTree: {},
    loadedPaths: new Set<string>(),
    pathItems: {},
    pathAliases: {},
    namespaceOwnedByPlugin: false,
    schemaEpoch: 0,
    expanded: new Set<string>(),
    selectedId: null,
    loading: false,
    ensuringCount: 0,
    error: null,
    columnInflight: new Set<string>(),
    ...overrides,
  }) as ConnectionSchemaState;

describe('rankConnections', () => {
  it('matches the four connection fields with an explainable match', () => {
    const conns = [
      connection({ id: 'name', name: 'Analytics' }),
      connection({ id: 'host', name: 'Other', host: 'analytics.example.com' }),
      connection({ id: 'database', name: 'Other 2', database: 'analytics' }),
      connection({ id: 'type', name: 'Other 3', databaseType: 'mysql' }),
    ];

    expect(rankConnections(conns, 'analytics').map((result) => result.connection.id)).toEqual([
      'name',
      'host',
      'database',
    ]);
    expect(rankConnections([conns[1]!], 'analytics')[0]?.match).toEqual({
      rank: 11,
      reason: 'host',
      context: 'analytics.example.com',
    });
    expect(rankConnections([conns[3]!], 'mysql')[0]?.match).toEqual({
      rank: 30,
      reason: 'database-type',
      context: 'mysql',
    });
  });

  it('preserves cached schema, table, path, and database-object matching', () => {
    const result = rankConnections(
      [connection({ id: 'cached' })],
      'function_name',
      {
        activeConnections: {
          cached: { status: 'connected', dbSessionId: 'session-1' },
        },
        schemas: new Map([
          [
            'session-1',
            schemaState({
              schemaNames: ['public'],
              namespaceTree: { public: { users: [] } },
              pathItems: { catalog: [{ name: 'path_table', tableType: 'table' }] },
            }),
          ],
        ]),
        dbTablesMap: {
          'session-1::app': [{ name: 'cached_table', tableType: 'table', schema: 'public' }],
        },
        dbObjectsMap: {
          'cached::app::functions': [{ kind: 'function', name: 'function_name', schema: 'public' }],
        },
      },
    );

    expect(result).toHaveLength(1);
    expect(result[0]?.match).toEqual({
      rank: 50,
      reason: 'object',
      context: 'app.public.function_name',
    });
  });

  it('uses match strength, pinned, recency, group, name, and id as stable keys', () => {
    const result = rankConnections([
      connection({ id: 'name-prefix', name: 'Prod app', pinned: true }),
      connection({ id: 'name-exact', name: 'prod' }),
      connection({
        id: 'recent',
        name: 'Recent app',
        lastConnectedAt: '2026-08-30T10:00:00Z',
      }),
      connection({ id: 'old', name: 'Old app', lastConnectedAt: '2026-08-29T10:00:00Z' }),
    ], 'prod');

    expect(result.map(({ connection: item }) => item.id)).toEqual(['name-exact', 'name-prefix']);

    const noSearch = rankConnections([
      connection({ id: 'recent', name: 'Zed', lastConnectedAt: '2026-08-30T10:00:00Z' }),
      connection({ id: 'pinned', name: 'Zulu', pinned: true }),
      connection({ id: 'name', name: 'Alpha', group: 'dev' }),
      connection({ id: 'older', name: 'Beta', lastConnectedAt: '2026-08-29T10:00:00Z' }),
    ], '');
    expect(noSearch.map(({ connection: item }) => item.id)).toEqual([
      'pinned',
      'recent',
      'older',
      'name',
    ]);
  });

  it('deduplicates connection ids in ranked search results', () => {
    const duplicate = connection({ id: 'same', name: 'Same' });
    expect(rankConnections([duplicate, { ...duplicate }], 'same')).toHaveLength(1);
  });
});

describe('groupConnectionsWithRecentSections', () => {
  it('keeps pinned, recent, and group sections without duplicate connections', () => {
    const conns = [
      connection({ id: 'pinned', name: 'Pinned', pinned: true, group: 'prod' }),
      connection({
        id: 'recent',
        name: 'Recent',
        group: 'prod',
        lastConnectedAt: '2026-08-30T10:00:00Z',
      }),
      connection({ id: 'grouped', name: 'Grouped', group: 'dev' }),
    ];

    const sections = groupConnectionsWithRecentSections(conns, ['prod', 'dev']);
    expect(sections.map((section) => section.group)).toEqual([
      PINNED_GROUP_KEY,
      RECENT_GROUP_KEY,
      'dev',
    ]);
    expect(sections.flatMap((section) => section.connections).map((item) => item.id)).toEqual([
      'pinned',
      'recent',
      'grouped',
    ]);
  });
});
