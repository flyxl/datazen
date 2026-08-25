import { databaseCommands } from '../commands/database';
import { queryCommands } from '../commands/query';
import type { TableSchema } from '../types';

const CACHE_TTL = 60_000; // 60 seconds

interface CacheEntry<T> {
  data: T;
  timestamp: number;
}

const schemaCache = new Map<string, CacheEntry<TableSchema>>();
const ddlCache = new Map<string, CacheEntry<string>>();

function cacheKey(dbSessionId: string, tableName: string): string {
  return `${dbSessionId}::${tableName}`;
}

function isValid<T>(entry: CacheEntry<T> | undefined): entry is CacheEntry<T> {
  return !!entry && Date.now() - entry.timestamp < CACHE_TTL;
}

export async function getCachedTableSchema(
  dbSessionId: string,
  tableName: string,
): Promise<TableSchema> {
  const key = cacheKey(dbSessionId, tableName);
  const cached = schemaCache.get(key);
  if (isValid(cached)) return cached.data;

  const data = await databaseCommands.getTableSchema(dbSessionId, tableName);
  schemaCache.set(key, { data, timestamp: Date.now() });
  return data;
}

export async function getCachedDDL(
  dbSessionId: string,
  tableName: string,
  sql: string,
  resultExtractor: (rows: unknown[][]) => string,
): Promise<string> {
  const key = cacheKey(dbSessionId, tableName);
  const cached = ddlCache.get(key);
  if (isValid(cached)) return cached.data;

  const multi = await queryCommands.executeQuery(dbSessionId, sql);
  const row = multi.results[0]?.rows[0];
  const data = resultExtractor(row ? [row] : []);
  ddlCache.set(key, { data, timestamp: Date.now() });
  return data;
}

export function invalidateSchemaCache(dbSessionId: string, tableName?: string): void {
  if (tableName) {
    const key = cacheKey(dbSessionId, tableName);
    schemaCache.delete(key);
    ddlCache.delete(key);
  } else {
    for (const k of [...schemaCache.keys()]) {
      if (k.startsWith(`${dbSessionId}::`)) schemaCache.delete(k);
    }
    for (const k of [...ddlCache.keys()]) {
      if (k.startsWith(`${dbSessionId}::`)) ddlCache.delete(k);
    }
  }
}
