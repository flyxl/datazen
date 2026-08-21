import { describe, expect, it } from 'vitest';
import {
  filterTableItems,
  getObjectFilter,
  isSystemDatabaseName,
  isSystemSchemaName,
  matchNamePattern,
  matchesTableNameFilter,
  shouldShowDatabase,
  shouldShowSchema,
} from '../objectFilter';
import type { ConnectionConfig, TableInfo } from '../../types';

function conn(options?: ConnectionConfig['options']): ConnectionConfig {
  return {
    id: 'c1',
    name: 'Test',
    databaseType: 'postgresql',
    sslMode: 'disable',
    options,
  };
}

describe('objectFilter', () => {
  it('reads filter prefs from connection options', () => {
    expect(
      getObjectFilter(
        conn({
          objectFilter: {
            hideSystemSchemas: true,
            tableNameInclude: 'app_*',
            tableNameExclude: 'tmp*',
          },
        }),
      ),
    ).toEqual({
      hideSystemSchemas: true,
      tableNameInclude: 'app_*',
      tableNameExclude: 'tmp*',
    });
  });

  it('matches glob and substring patterns', () => {
    expect(matchNamePattern('app_users', 'app_*')).toBe(true);
    expect(matchNamePattern('users', 'app')).toBe(false);
    expect(matchNamePattern('users_backup', 'backup')).toBe(true);
  });

  it('filters table names with include/exclude', () => {
    const filter = { tableNameInclude: 'app_*', tableNameExclude: '*_tmp' };
    expect(matchesTableNameFilter('app_users', filter)).toBe(true);
    expect(matchesTableNameFilter('app_users_tmp', filter)).toBe(false);
    expect(matchesTableNameFilter('legacy', filter)).toBe(false);
  });

  it('hides system schemas and databases when enabled', () => {
    const filter = { hideSystemSchemas: true };
    expect(isSystemSchemaName('pg_catalog')).toBe(true);
    expect(isSystemDatabaseName('information_schema')).toBe(true);
    expect(shouldShowSchema('public', filter)).toBe(true);
    expect(shouldShowSchema('pg_catalog', filter)).toBe(false);
    expect(shouldShowDatabase('mydb', filter)).toBe(true);
    expect(shouldShowDatabase('mysql', filter)).toBe(false);
  });

  it('filterTableItems removes system tables and applies name filters', () => {
    const items: TableInfo[] = [
      { name: 'users', schema: 'public', tableType: 'table' },
      { name: 'pg_class', schema: 'pg_catalog', tableType: 'systemTable' },
      { name: 'app_events', schema: 'public', tableType: 'table' },
    ];
    const filtered = filterTableItems(items, {
      hideSystemSchemas: true,
      tableNameInclude: 'app_*',
    });
    expect(filtered.map((i) => i.name)).toEqual(['app_events']);
  });
});
