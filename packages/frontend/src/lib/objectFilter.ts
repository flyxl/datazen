import type { ConnectionConfig, TableInfo } from '../types';

export interface ObjectFilterPrefs {
  hideSystemSchemas?: boolean;
  tableNameInclude?: string;
  tableNameExclude?: string;
}

const SYSTEM_SCHEMAS = new Set([
  'information_schema',
  'pg_catalog',
  'pg_toast',
  'mysql',
  'performance_schema',
  'sys',
]);

const SYSTEM_DATABASES = new Set([
  'information_schema',
  'mysql',
  'performance_schema',
  'sys',
  'postgres',
  'template0',
  'template1',
]);

export function getObjectFilter(conn: ConnectionConfig): ObjectFilterPrefs {
  const raw = conn.options?.objectFilter;
  if (!raw || typeof raw !== 'object' || Array.isArray(raw)) return {};
  const record = raw as Record<string, unknown>;
  return {
    hideSystemSchemas:
      record.hideSystemSchemas === true
        ? true
        : record.hideSystemSchemas === false
          ? false
          : undefined,
    tableNameInclude:
      typeof record.tableNameInclude === 'string' ? record.tableNameInclude : undefined,
    tableNameExclude:
      typeof record.tableNameExclude === 'string' ? record.tableNameExclude : undefined,
  };
}

export function isSystemSchemaName(name: string): boolean {
  const lower = name.toLowerCase();
  if (SYSTEM_SCHEMAS.has(lower)) return true;
  return lower.startsWith('pg_');
}

export function isSystemDatabaseName(name: string): boolean {
  return SYSTEM_DATABASES.has(name.toLowerCase());
}

/** Match glob (* only) or simple substring when pattern has no wildcard. */
export function matchNamePattern(name: string, pattern: string): boolean {
  const p = pattern.trim();
  if (!p) return true;
  const n = name.toLowerCase();
  const lower = p.toLowerCase();
  if (lower.includes('*')) {
    const escaped = lower.replace(/[.+?^${}()|[\]\\]/g, '\\$&').replace(/\*/g, '.*');
    return new RegExp(`^${escaped}$`).test(n);
  }
  return n.includes(lower);
}

export function matchesTableNameFilter(name: string, filter: ObjectFilterPrefs): boolean {
  const include = filter.tableNameInclude?.trim();
  const exclude = filter.tableNameExclude?.trim();
  if (include && !matchNamePattern(name, include)) return false;
  if (exclude && matchNamePattern(name, exclude)) return false;
  return true;
}

export function shouldShowSchema(schemaName: string, filter: ObjectFilterPrefs): boolean {
  if (filter.hideSystemSchemas && isSystemSchemaName(schemaName)) return false;
  return true;
}

export function shouldShowDatabase(dbName: string, filter: ObjectFilterPrefs): boolean {
  if (filter.hideSystemSchemas && isSystemDatabaseName(dbName)) return false;
  return true;
}

export function filterTableItems(items: TableInfo[], filter: ObjectFilterPrefs): TableInfo[] {
  return items.filter((item) => {
    if (filter.hideSystemSchemas && item.tableType === 'systemTable') return false;
    if (item.schema && filter.hideSystemSchemas && isSystemSchemaName(item.schema)) return false;
    return matchesTableNameFilter(item.name, filter);
  });
}

export function withObjectFilterOptions(
  conn: ConnectionConfig,
  filter: ObjectFilterPrefs,
): ConnectionConfig {
  return {
    ...conn,
    options: {
      ...(conn.options ?? {}),
      objectFilter: filter,
    },
  };
}
