import { describe, expect, it } from 'vitest';
import { buildNavigatorFlatRows } from '../buildFlatRows';
import { createEmptyConnectionSchema } from '../../../../stores/schemaStoreState';
import type { ConnectionEntry } from '../../../../stores/activeConnectionStore';
import type { ConnectionConfig } from '../../../../types';
import type { UnifiedRow } from '../types';

function makeConnection(overrides: Partial<ConnectionConfig> = {}): ConnectionConfig {
  return {
    id: 'c1',
    name: 'MySQL',
    databaseType: 'mysql',
    sslMode: 'disable',
    ...overrides,
  };
}

function connectedEntry(): ConnectionEntry {
  return {
    dbSessionId: 'sess-1',
    connectionId: 'c1',
    status: 'connected',
    serverInfo: null,
    currentDatabase: null,
    error: null,
  };
}

function baseParams() {
  return {
    expandedGroups: new Set(['']),
    expandedConnections: new Set(['::c1']),
    expandedDbs: new Set<string>(),
    expandedSchemas: new Set<string>(),
    expandedCats: new Set<string>(),
    activeConnections: { c1: connectedEntry() } as Record<string, ConnectionEntry | undefined>,
    activeConnectionId: 'c1',
    dbTablesMap: {} as Record<string, never[]>,
    dbErrors: {} as Record<string, string>,
    dbObjectsMap: {} as Record<string, never[]>,
    loadingDbs: new Set<string>(),
    openDbs: {} as Record<string, Set<string>>,
    query: '',
    t: (key: string) => key,
  };
}

describe('buildNavigatorFlatRows db-error row', () => {
  it('replaces a failed database load with a visible error row', () => {
    const params = {
      ...baseParams(),
      grouped: [{ group: '', connections: [makeConnection()] }],
      expandedDbs: new Set(['c1::orders']),
      schemas: new Map([
        [
          'sess-1',
          {
            ...createEmptyConnectionSchema(),
            connectionId: 'c1',
            databaseType: 'mysql',
            databases: ['orders'],
          },
        ],
      ]),
      dbErrors: { 'sess-1::orders': 'connection refused' },
    };
    const rows = buildNavigatorFlatRows(params);

    const errorRow = rows.find((row) => row.type === 'db-error');
    expect(errorRow).toBeDefined();
    expect(errorRow).toMatchObject({
      type: 'db-error',
      ownerKey: 'db:c1::orders',
      message: 'connection refused',
    });
    // The error is the database's only child: no empty category list below it.
    const afterDb = rows.findIndex((row) => row.type === 'db');
    const between = rows.slice(afterDb + 1);
    expect(between.some((row) => row.type === 'category')).toBe(false);
    expect(between[0]).toBe(errorRow);
  });

  it('keeps showing tables when a refresh failed but a cached list exists', () => {
    const params = {
      ...baseParams(),
      grouped: [{ group: '', connections: [makeConnection()] }],
      expandedDbs: new Set(['c1::orders']),
      schemas: new Map([
        [
          'sess-1',
          {
            ...createEmptyConnectionSchema(),
            connectionId: 'c1',
            databaseType: 'mysql',
            databases: ['orders'],
          },
        ],
      ]),
      dbErrors: { 'sess-1::orders': 'stale refresh failed' },
      dbTablesMap: {
        'sess-1::orders': [
          {
            name: 'invoices',
            tableType: 'table' as const,
          },
        ] as never[],
      },
    };
    const rows = buildNavigatorFlatRows(params);
    expect(rows.some((row) => row.type === 'db-error')).toBe(false);
    expect(rows.some((row) => row.type === 'category')).toBe(true);
  });

  it('shows the session-level error for a single-database connection', () => {
    const params = {
      ...baseParams(),
      grouped: [{ group: '', connections: [makeConnection({ database: 'shop' })] }],
      expandedDbs: new Set(['c1::shop']),
      schemas: new Map([
        [
          'sess-1',
          {
            ...createEmptyConnectionSchema(),
            connectionId: 'c1',
            databaseType: 'mysql',
            currentDatabase: 'shop',
            error: 'Typed command result unavailable',
          },
        ],
      ]),
    };
    const rows = buildNavigatorFlatRows(params);
    const errorRow = rows.find((row) => row.type === 'db-error');
    expect(errorRow).toMatchObject({
      type: 'db-error',
      ownerKey: 'db:c1::shop',
      message: 'Typed command result unavailable',
    });
  });

  it('renders no error row while the load is still in flight', () => {
    const params = {
      ...baseParams(),
      grouped: [{ group: '', connections: [makeConnection()] }],
      expandedDbs: new Set(['c1::orders']),
      loadingDbs: new Set(['sess-1::orders']),
      schemas: new Map([
        [
          'sess-1',
          {
            ...createEmptyConnectionSchema(),
            connectionId: 'c1',
            databaseType: 'mysql',
            databases: ['orders'],
          },
        ],
      ]),
      dbErrors: { 'sess-1::orders': 'a previous attempt failed' },
    };
    const rows = buildNavigatorFlatRows(params);
    expect(rows.some((row: UnifiedRow) => row.type === 'db-error')).toBe(false);
    expect(rows.some((row) => row.type === 'db-loading')).toBe(true);
  });
});
