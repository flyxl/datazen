import { describe, expect, it } from 'vitest';
import {
  filterSchemaObjectsByType,
  groupSchemaObjects,
  matchingColumns,
  searchSchemaObjects,
  tableMatchesObjectSearch,
  type SchemaObjectIndexEntry,
} from '../schemaObjectSearch';
import type { DatabaseType } from '../../types';

const index: SchemaObjectIndexEntry[] = [
  {
    connectionId: 'conn-1',
    dbSessionId: 'session-1',
    databaseType: 'postgresql' as DatabaseType,
    connectionName: 'Primary PG',
    host: 'app-db.internal',
    database: 'app',
    tables: [
      { name: 'users', schema: 'public', tableType: 'table' },
      { name: 'active_users', schema: 'public', tableType: 'view' },
    ],
    columnMap: {
      users: ['id', 'EmailAddress'],
      'public.active_users': ['id', 'email'],
    },
    objects: [{ kind: 'function', schema: 'public', name: 'refresh_users' }],
  },
  {
    connectionId: 'conn-1',
    dbSessionId: 'session-1',
    databaseType: 'postgresql' as DatabaseType,
    connectionName: 'Primary PG',
    database: 'app',
    schema: 'audit',
    tables: [{ name: 'events', tableType: 'table' }],
    columnMap: { events: ['event_id', 'created_at'] },
    objects: [{ kind: 'procedure', name: 'compact_events' }],
  },
  {
    connectionId: 'conn-2',
    dbSessionId: 'session-2',
    databaseType: 'mysql' as DatabaseType,
    connectionName: 'Reporting MySQL',
    database: 'reporting',
    schema: 'main',
    views: [{ name: 'sales_summary', tableType: 'view' }],
  },
];

describe('schemaObjectSearch', () => {
  it('matches table names', () => {
    expect(tableMatchesObjectSearch('users', 'user')).toBe(true);
    expect(tableMatchesObjectSearch('orders', 'user')).toBe(false);
  });

  it('matches columns when query length >= 2', () => {
    expect(tableMatchesObjectSearch('orders', 'email', ['id', 'email'])).toBe(true);
    expect(tableMatchesObjectSearch('orders', 'em', ['id', 'email'])).toBe(true);
    // Single-char queries only match table/view names, not columns.
    expect(tableMatchesObjectSearch('orders', 'x', ['id', 'email', 'xyz'])).toBe(false);
    expect(tableMatchesObjectSearch('orders', 'phone', ['id', 'email'])).toBe(false);
  });

  it('lists matching columns', () => {
    expect(matchingColumns('id', ['user_id', 'id', 'name'])).toEqual(['user_id', 'id']);
  });

  it('searches loaded slices across connections, databases, and schemas', () => {
    const results = searchSchemaObjects({ entries: index }, 'EMAIL');

    expect(results).toHaveLength(2);
    expect(results.map((result) => [result.objectType, result.name, result.tableName])).toEqual([
      ['column', 'email', 'active_users'],
      ['column', 'EmailAddress', 'users'],
    ]);
    expect(results[0]).toMatchObject({
      connectionId: 'conn-1',
      dbSessionId: 'session-1',
      database: 'app',
      schema: 'public',
    });
  });

  it('retains all context fields that caused a result to match', () => {
    const result = searchSchemaObjects(index, 'app').find(
      (candidate) => candidate.objectType === 'table' && candidate.name === 'users',
    );

    expect(result).toMatchObject({
      connectionName: 'Primary PG',
      host: 'app-db.internal',
      database: 'app',
      matchedFields: ['host', 'database'],
      matchReason: 'host',
    });
  });

  it('distinguishes column-name hits from owning-table hits', () => {
    const columnNameHit = searchSchemaObjects(index, 'email').find(
      (result) => result.objectType === 'column' && result.name === 'EmailAddress',
    );
    expect(columnNameHit).toMatchObject({
      tableName: 'users',
      matchedFields: ['column'],
      matchReason: 'column',
    });

    const owningTableHit = searchSchemaObjects(index, 'users').find(
      (result) => result.objectType === 'column' && result.tableName === 'users',
    );
    expect(owningTableHit).toMatchObject({
      matchedFields: ['table'],
      matchReason: 'table',
    });
  });

  it('retains database type as a searchable match field', () => {
    const results = searchSchemaObjects(index, 'postgresql');

    expect(results.length).toBeGreaterThan(0);
    expect(results.every((result) => result.matchedFields.includes('type'))).toBe(true);
    expect(results.every((result) => result.matchReason === 'type')).toBe(true);
  });

  it('returns all loaded object types for an empty query and preserves object context', () => {
    const results = searchSchemaObjects(index, '   ');

    expect(results.some((result) => result.objectType === 'table')).toBe(true);
    expect(results.some((result) => result.objectType === 'view')).toBe(true);
    expect(results.some((result) => result.objectType === 'column')).toBe(true);
    expect(results.some((result) => result.objectType === 'function')).toBe(true);
    expect(results.some((result) => result.objectType === 'routine')).toBe(true);
    expect(results.every((result) => result.matchedFields.length === 0)).toBe(true);
    expect(results.every((result) => result.matchReason === undefined)).toBe(true);

    const eventColumn = results.find(
      (result) => result.objectType === 'column' && result.name === 'event_id',
    );
    expect(eventColumn).toMatchObject({
      connectionId: 'conn-1',
      dbSessionId: 'session-1',
      database: 'app',
      schema: 'audit',
      tableName: 'events',
    });
  });

  it('filters by object type and treats routine as function/procedure', () => {
    const results = searchSchemaObjects(index, '');

    expect(filterSchemaObjectsByType(results, ['view']).map((result) => result.name)).toEqual([
      'active_users',
      'sales_summary',
    ]);
    expect(
      filterSchemaObjectsByType(results, ['routine']).map((result) => result.name),
    ).toEqual(['compact_events', 'refresh_users']);
    expect(
      searchSchemaObjects(index, '', { database: 'reporting', schema: 'main' }).map(
        (result) => result.name,
      ),
    ).toEqual(['sales_summary']);
  });

  it('groups by connection, database, schema, and object type', () => {
    const groups = groupSchemaObjects(searchSchemaObjects(index, ''));

    expect(groups).toHaveLength(8);
    expect(
      groups.map((group) => [group.connectionId, group.database, group.schema]),
    ).toContainEqual(['conn-1', 'app', 'audit']);
    expect(
      groups.find(
        (group) =>
          group.connectionId === 'conn-1' &&
          group.schema === 'public' &&
          group.objectType === 'column',
      )?.results.map((result) => result.tableName),
    ).toEqual(['active_users', 'active_users', 'users', 'users']);
  });
});
