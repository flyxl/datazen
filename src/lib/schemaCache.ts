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

function cacheKey(connectionId: string, tableName: string): string {
  return `${connectionId}::${tableName}`;
}

function isValid<T>(entry: CacheEntry<T> | undefined): entry is CacheEntry<T> {
  return !!entry && Date.now() - entry.timestamp < CACHE_TTL;
}

export async function getCachedTableSchema(
  connectionId: string,
  tableName: string,
): Promise<TableSchema> {
  const key = cacheKey(connectionId, tableName);
  const cached = schemaCache.get(key);
  if (isValid(cached)) return cached.data;

  const data = await databaseCommands.getTableSchema(connectionId, tableName);
  schemaCache.set(key, { data, timestamp: Date.now() });
  return data;
}

export async function getCachedDDL(
  connectionId: string,
  tableName: string,
  sql: string,
  resultExtractor: (rows: unknown[][]) => string,
): Promise<string> {
  const key = cacheKey(connectionId, tableName);
  const cached = ddlCache.get(key);
  if (isValid(cached)) return cached.data;

  const multi = await queryCommands.executeQuery(connectionId, sql);
  const row = multi.results[0]?.rows[0];
  const data = resultExtractor(row ? [row] : []);
  ddlCache.set(key, { data, timestamp: Date.now() });
  return data;
}

export function invalidateSchemaCache(connectionId: string, tableName?: string): void {
  if (tableName) {
    const key = cacheKey(connectionId, tableName);
    schemaCache.delete(key);
    ddlCache.delete(key);
  } else {
    for (const k of [...schemaCache.keys()]) {
      if (k.startsWith(`${connectionId}::`)) schemaCache.delete(k);
    }
    for (const k of [...ddlCache.keys()]) {
      if (k.startsWith(`${connectionId}::`)) ddlCache.delete(k);
    }
  }
}
