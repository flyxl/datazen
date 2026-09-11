import { databaseCommands } from '../commands/database';
import { queryCommands } from '../commands/query';
import type { TableSchema } from '../types';

const CACHE_TTL = 60_000; // 60 seconds
const ERROR_TTL = 10_000; // short-lived error suppression to avoid re-request storms

interface CacheEntry<T> {
  data: T;
  timestamp: number;
}

interface ErrorEntry {
  error: unknown;
  timestamp: number;
}

const schemaCache = new Map<string, CacheEntry<TableSchema>>();
const ddlCache = new Map<string, CacheEntry<string>>();
const schemaInflight = new Map<string, Promise<TableSchema>>();
const ddlInflight = new Map<string, Promise<string>>();
const schemaErrors = new Map<string, ErrorEntry>();
const ddlErrors = new Map<string, ErrorEntry>();

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

function cacheKey(dbSessionId: string, tableName: string, database?: string | null): string {
  return database ? `${dbSessionId}::${database}::${tableName}` : `${dbSessionId}::${tableName}`;
}

/** Optional DDL cache identity. When supplied, the key discriminates object
 * kind and full namespace so tables/views / cross-schema relations don't collide. */
export interface DdlCacheIdentity {
  objectKind?: 'table' | 'view';
  namespacePath?: readonly string[];
}

function ddlCacheKey(dbSessionId: string, tableName: string, identity?: DdlCacheIdentity): string {
  const kind = identity?.objectKind ?? 'table';
  const ns = identity?.namespacePath?.length ? `${identity.namespacePath.join('.')}.` : '';
  return `${dbSessionId}::${kind}::${ns}${tableName}`;
}

function isValid<T>(entry: CacheEntry<T> | undefined): entry is CacheEntry<T> {
  return !!entry && Date.now() - entry.timestamp < CACHE_TTL;
}

function isErrorFresh(entry: ErrorEntry | undefined): boolean {
  return !!entry && Date.now() - entry.timestamp < ERROR_TTL;
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
  database?: string | null,
): Promise<TableSchema> {
  const key = cacheKey(dbSessionId, tableName, database);

  const cached = schemaCache.get(key);
  if (isValid(cached)) return cached.data;

  const err = schemaErrors.get(key);
  if (err && isErrorFresh(err)) {
    throw err.error;
  }

  let inflight = schemaInflight.get(key);
  if (!inflight) {
    inflight = databaseCommands
      .getTableSchema(dbSessionId, tableName, database)
      .then((data) => {
        const frozen = deepFreeze(data);
        schemaCache.set(key, { data: frozen, timestamp: Date.now() });
        schemaErrors.delete(key);
        return frozen;
      })
      .catch((error: unknown) => {
        schemaErrors.set(key, { error, timestamp: Date.now() });
        throw error;
      })
      .finally(() => {
        schemaInflight.delete(key);
      });
    schemaInflight.set(key, inflight);
  }
  return inflight;
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
  identity?: DdlCacheIdentity,
  database?: string | null,
): Promise<string> {
  const key = ddlCacheKey(dbSessionId, tableName, identity);

  const cached = ddlCache.get(key);
  if (isValid(cached)) return cached.data;

  const err = ddlErrors.get(key);
  if (err && isErrorFresh(err)) {
    throw err.error;
  }

  let inflight = ddlInflight.get(key);
  if (!inflight) {
    inflight = (async () => {
      // Pin the session to `database` before running (mirrors query pinning in
      // queryCommands.executeQuery's F1 path) so a copy-DDL call never resolves
      // against a stale/active database that may not own the relation.
      const multi = await queryCommands.executeQuery(
        dbSessionId,
        sql,
        undefined,
        database ?? null,
        null,
      );
      const row = multi.results[0]?.rows[0];
      const data = resultExtractor(row ? [row] : []);
      ddlCache.set(key, { data, timestamp: Date.now() });
      ddlErrors.delete(key);
      return data;
    })()
      .catch((error: unknown) => {
        ddlErrors.set(key, { error, timestamp: Date.now() });
        throw error;
      })
      .finally(() => {
        ddlInflight.delete(key);
      });
    ddlInflight.set(key, inflight);
  }
  return inflight;
}

export interface DdlCacheInvalidateScope {
  objectKind?: 'table' | 'view';
  namespacePath?: readonly string[];
}

export function invalidateSchemaCache(
  dbSessionId?: string,
  tableName?: string,
  identity?: DdlCacheInvalidateScope,
): void {
  if (!dbSessionId) {
    schemaCache.clear();
    schemaErrors.clear();
    ddlCache.clear();
    ddlErrors.clear();
    return;
  }
  if (tableName) {
    const key = cacheKey(dbSessionId, tableName);
    const dkey = ddlCacheKey(dbSessionId, tableName, identity);
    schemaCache.delete(key);
    schemaErrors.delete(key);
    for (const k of [...schemaCache.keys()]) {
      if (k.startsWith(`${dbSessionId}::`) && (k.endsWith(`::${tableName}`) || k === key)) {
        schemaCache.delete(k);
      }
    }
    for (const k of [...schemaErrors.keys()]) {
      if (k.startsWith(`${dbSessionId}::`) && (k.endsWith(`::${tableName}`) || k === key)) {
        schemaErrors.delete(k);
      }
    }
    ddlCache.delete(dkey);
    ddlErrors.delete(dkey);
  } else {
    for (const k of [...schemaCache.keys()]) {
      if (k.startsWith(`${dbSessionId}::`)) schemaCache.delete(k);
    }
    for (const k of [...schemaErrors.keys()]) {
      if (k.startsWith(`${dbSessionId}::`)) schemaErrors.delete(k);
    }
    for (const k of [...ddlCache.keys()]) {
      if (k.startsWith(`${dbSessionId}::`)) ddlCache.delete(k);
    }
    for (const k of [...ddlErrors.keys()]) {
      if (k.startsWith(`${dbSessionId}::`)) ddlErrors.delete(k);
    }
  }
  notifyInvalidation(dbSessionId, tableName);
}
