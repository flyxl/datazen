import { DB_REGISTRY } from '../databaseTypes';
import type { DatabaseTypeMeta } from '../databaseMeta';
import type { DatabaseType } from '../../types';

export interface ResolveCreateTableSchemaParams {
  currentDatabase: string | null;
  /** PG schema namespace from sidebar selection (table.schema), when known. */
  contextSchema?: string | null;
}

/**
 * Schema qualifier for CREATE TABLE planning.
 * - PostgreSQL: schema namespace (e.g. public), not the database name.
 * - MySQL / default-sql: logical database name.
 * - SQLite / path DBs: unqualified (null).
 */
export function resolveCreateTableSchemaFromMeta(
  meta: DatabaseTypeMeta | undefined,
  databaseType: string,
  params: ResolveCreateTableSchemaParams,
): string | null {
  if (!meta) {
    if (databaseType === 'postgresql') {
      return params.contextSchema ?? 'public';
    }
    return params.contextSchema ?? params.currentDatabase ?? null;
  }

  const namespaceEnsure =
    meta.namespaceEnsure ?? (databaseType === 'postgresql' ? 'postgresql' : 'default-sql');

  if (namespaceEnsure === 'postgresql') {
    // PG schema namespace — never use the logical database name as schema.
    return params.contextSchema ?? 'public';
  }

  if (meta.databaseFieldType === 'path') {
    return null;
  }

  return params.currentDatabase ?? null;
}

export function resolveCreateTableSchema(
  databaseType: string,
  params: ResolveCreateTableSchemaParams,
): string | null {
  return resolveCreateTableSchemaFromMeta(
    DB_REGISTRY[databaseType as DatabaseType],
    databaseType,
    params,
  );
}
