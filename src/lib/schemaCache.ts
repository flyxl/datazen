import type { TableSchema } from '../types';

import { relationKey } from '@datazen/driver-sdk';
import { executeTargetCommand, metadataTarget, readTargetSchema, metadataScopeKey } from './session/targetCommand';
import { SchemaResourceCache } from './schemaResourceCache';

const schemaCache = new SchemaResourceCache<TableSchema>();
const ddlCache = new SchemaResourceCache<string>();

/** Observers notified on every schema/DDL invalidation (downstream caches react). */
type InvalidationListener = (dbSessionId: string, tableName?: string) => void;
const invalidationListeners = new Set<InvalidationListener>();

/** Subscribe to schema/DDL cache invalidations (for downstream metadata caches). */
export function subscribeSchemaInvalidation(listener: InvalidationListener): () => void {
  invalidationListeners.add(listener);
  return () => {
    invalidationListeners.delete(listener);
  };
}

function notifyInvalidation(dbSessionId: string, tableName?: string): void {
  for (const listener of [...invalidationListeners]) listener(dbSessionId, tableName);
}

function cacheKey(
  dbSessionId: string,
  tableName: string,
  database: string,
  schema?: string | null,
): string {
  return relationKey({ dbSessionId, database, schema: schema ?? null, name: tableName });
}

/** Optional DDL cache identity. When supplied, the key discriminates object
 * kind and full namespace so tables/views / cross-schema relations don't collide. */
export interface DdlCacheIdentity {
  objectKind?: 'table' | 'view';
  namespacePath?: readonly string[];
}

function ddlCacheKey(
  dbSessionId: string,
  tableName: string,
  database: string,
  identity?: DdlCacheIdentity,
): string {
  return JSON.stringify([
    dbSessionId,
    database,
    identity?.objectKind ?? 'table',
    identity?.namespacePath ?? [],
    tableName,
  ]);
}

function deepFreeze<T>(value: T): T {
  if (value == null || typeof value !== 'object') return value;
  const seen = new WeakSet<object>();
  const walk = (v: unknown) => {
    if (v == null || typeof v !== 'object' || seen.has(v as object)) return;
    seen.add(v as object);
    for (const key of Object.keys(v as object)) walk((v as Record<string, unknown>)[key]);
    Object.freeze(v);
  };
  walk(value);
  return value;
}

/**
 * Fetch and cache a relation schema, deduplicating concurrent requests and
 * transiently caching failures. The returned object is a deep-frozen immutable
 * snapshot consumers may read but never mutate.
 */
export async function getCachedTableSchema(
  dbSessionId: string,
  tableName: string,
  database: string,
  schema?: string | null,
): Promise<TableSchema> {
  const key = JSON.stringify([await metadataScopeKey(dbSessionId), cacheKey(dbSessionId, tableName, database, schema)]);

  return schemaCache.get(key, async () =>
    deepFreeze(await readTargetSchema(dbSessionId, tableName, database, schema ?? null)),
  );
}

/**
 * Fetch and cache an object's DDL via a dialect-generated query + extractor.
 * Concurrent identical requests are deduplicated and failures are transiently
 * suppressed. Supply `identity` when the caller knows the object kind and full
 * namespace so `table` vs `view` / cross-schema objects never share a cache cell.
 */
export async function getCachedDDL(
  dbSessionId: string,
  tableName: string,
  sql: string,
  resultExtractor: (rows: unknown[][]) => string,
  database: string,
  identity?: DdlCacheIdentity,
): Promise<string> {
  const key = JSON.stringify([await metadataScopeKey(dbSessionId), ddlCacheKey(dbSessionId, tableName, database, identity)]);

  return ddlCache.get(key, async () => {
    const rows = await executeTargetCommand(metadataTarget(dbSessionId, database, null, tableName), { command: 'query', input: { sql } }, 'rows') as unknown[][];
    return resultExtractor(rows);
  });
}

export interface DdlCacheInvalidateScope {
  objectKind?: 'table' | 'view';
  namespacePath?: readonly string[];
}

export function invalidateSchemaCache(
  dbSessionId?: string,
  tableName?: string,
  identity?: DdlCacheInvalidateScope,
  database?: string,
): void {
  const sessions = new Set<string>();
  const matches = (key: string, ddl: boolean): boolean => {
    const parsed: unknown = JSON.parse(key);
    const parts: unknown = Array.isArray(parsed) && parsed.length === 2 && typeof parsed[1] === 'string'
      ? JSON.parse(parsed[1]) : parsed;
    if (!Array.isArray(parts)) return false;
    const [session, db, kind, namespace, name] = parts;
    if (typeof session !== 'string') return false;
    if (dbSessionId && session !== dbSessionId) return false;
    if (database && db !== database) return false;
    const relationName = ddl ? name : namespace;
    if (tableName && relationName !== tableName) return false;
    if (ddl && identity?.objectKind && kind !== identity.objectKind) return false;
    if (
      ddl &&
      identity?.namespacePath &&
      JSON.stringify(namespace) !== JSON.stringify(identity.namespacePath)
    )
      return false;
    sessions.add(session);
    return true;
  };
  schemaCache.invalidate((key) => matches(key, false));
  ddlCache.invalidate((key) => matches(key, true));
  if (dbSessionId) notifyInvalidation(dbSessionId, tableName);
  else for (const session of sessions) notifyInvalidation(session);
}
