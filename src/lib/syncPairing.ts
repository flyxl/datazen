/**
 * Sync pairing policy for the Data Sync window (mirrors `src-tauri/src/sync/pairing.rs`).
 */

import { DB_REGISTRY } from './databaseTypes';
import type { DatabaseTypeMeta } from './databaseMeta';

function registryMeta(dbType: string): DatabaseTypeMeta | undefined {
  return (DB_REGISTRY as Record<string, DatabaseTypeMeta | undefined>)[dbType];
}

export type SyncPath = 'direct' | 'ir' | 'unsupported';

export interface SyncPairingResult {
  path: SyncPath;
  supported: boolean;
  family?: string;
}

type SyncCategory = 'sql' | 'document' | 'kv' | 'other';

function syncCategory(dbType: string): SyncCategory {
  switch (dbType.toLowerCase()) {
    case 'redis':
      return 'kv';
    case 'mongodb':
      return 'document';
    case 'kiwi':
    case 'superset':
      return 'other';
    default: {
      const meta = registryMeta(dbType);
      if (meta?.category === 'kv') return 'kv';
      if (meta?.category === 'document') return 'document';
      return 'sql';
    }
  }
}

/** Normalize a database type id to its sync dialect family. */
export function normalizeSyncFamily(dbType: string): string {
  const meta = registryMeta(dbType);
  if (meta?.sqlDialect) return meta.sqlDialect;

  switch (dbType.toLowerCase()) {
    case 'postgres':
    case 'postgresql':
    case 'cloudberry':
    case 'questdb':
      return 'postgresql';
    case 'mysql':
    case 'mariadb':
    case 'tidb':
    case 'oceanbase':
    case 'doris':
    case 'starrocks':
    case 'manticore':
    case 'ob_oracle':
      return 'mysql';
    case 'sqlite':
    case 'rqlite':
    case 'turso':
      return 'sqlite';
    case 'sqlserver':
    case 'mssql':
      return 'sqlserver';
    case 'trino':
    case 'presto':
      return 'trino';
    default:
      return dbType.toLowerCase();
  }
}

/** Classify how a source/target database type pair should sync. */
export function resolveSyncPairing(sourceType: string, targetType: string): SyncPairingResult {
  const srcCat = syncCategory(sourceType);
  const tgtCat = syncCategory(targetType);

  if (srcCat !== tgtCat) {
    return { path: 'unsupported', supported: false };
  }

  if (srcCat === 'other') {
    return { path: 'unsupported', supported: false };
  }

  const srcFamily = normalizeSyncFamily(sourceType);
  const tgtFamily = normalizeSyncFamily(targetType);

  if (srcFamily === tgtFamily) {
    return { path: 'direct', supported: true, family: srcFamily };
  }

  if (srcCat === 'sql') {
    return { path: 'ir', supported: true };
  }

  return { path: 'unsupported', supported: false };
}

/** Whether target is selectable for sync given a source connection type. */
export function isSyncTargetSupported(sourceType: string, targetType: string): boolean {
  return resolveSyncPairing(sourceType, targetType).supported;
}
