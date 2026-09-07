import { getCachedDDL, type DdlCacheIdentity } from '../../../lib/schemaCache';
import { getSqlDialect, type SqlDialectStrategy } from '../../../lib/sqlDialects';
import type { DatabaseType } from '../../../types';
import type { QualifiedRelationId } from '../semantic/types';
import type { EditorRelationKind } from './types';

export interface CopyRelationDdlDeps {
  databaseType: DatabaseType;
  /** Drivers that cannot resolve a qualified target should set `allowQualified: false`. */
  allowQualified?: boolean;
  /** Injectable for tests (defaults to the real `getCachedDDL`). */
  getDdl?: typeof getCachedDDL;
  /** Injectable dialect resolver (defaults to `getSqlDialect`). */
  getDialect?: (databaseType: DatabaseType) => SqlDialectStrategy | null;
}

const DEFAULT_EXTRACTOR = (rows: unknown[][], extractColumnIndex: number): string => {
  const row = rows[0];
  const val = row?.[extractColumnIndex];
  return typeof val === 'string' ? val : val != null ? String(val) : '';
};

/** Resolve the dialect DDL query + extractor-column index strategy for a relation. */
export function resolveDdlQuery(
  dialect: SqlDialectStrategy | null,
  name: string,
  kind: EditorRelationKind,
  schemaRef?: string,
): { sql: string; extractColumnIndex: number; supported: boolean } {
  if (!dialect?.ddl) return { sql: '', extractColumnIndex: 0, supported: false };
  if (kind === 'view' && dialect.ddl.getViewDdlQuery) {
    return { ...dialect.ddl.getViewDdlQuery(name, schemaRef), supported: true };
  }
  if (dialect.ddl.getTableDdlQuery) {
    return { ...dialect.ddl.getTableDdlQuery(name, schemaRef), supported: true };
  }
  return { sql: '', extractColumnIndex: 0, supported: false };
}

/**
 * Copy a relation's DDL, reusing the same dialect SQL generation and extractor
 * strategy as DDLView. The identity (object kind + full namespace) is passed so
 * the upgraded `getCachedDDL` keeps tables/views and cross-schema objects in
 * separate cache cells. Throws when no DDL dialect exists.
 */
export async function copyRelationDdl(
  dbSessionId: string,
  identity: QualifiedRelationId,
  kind: EditorRelationKind,
  deps: CopyRelationDdlDeps,
): Promise<string> {
  const { databaseType } = deps;
  const getDdl = deps.getDdl ?? getCachedDDL;
  const getDialect = deps.getDialect ?? getSqlDialect;
  const dialect = getDialect(databaseType);
  const name = identity.name.name;
  const schemaRef =
    deps.allowQualified !== false && identity.namespacePath.length > 0
      ? identity.namespacePath.map((s) => s.name).join('.')
      : undefined;

  const { sql, extractColumnIndex, supported } = resolveDdlQuery(dialect, name, kind, schemaRef);
  if (!supported) throw new Error('unable-generate-ddl');

  const cacheIdentity: DdlCacheIdentity = {
    objectKind: kind,
    namespacePath: schemaRef ? schemaRef.split('.') : identity.namespacePath.map((s) => s.name),
  };

  return getDdl(
    dbSessionId,
    name,
    sql,
    (rows) => DEFAULT_EXTRACTOR(rows, extractColumnIndex),
    cacheIdentity,
  );
}
