/**
 * Data Sync pairing — backend is the single source of truth via `classify_data_sync_pair` IPC.
 * `syncCategory` / `normalizeSyncFamily` remain for Transfer UI gating (`transferPairing.ts`).
 */

import { useEffect, useState } from 'react';
import { syncCommands } from '../commands/sync';
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
  reason?: string;
}

type SyncCategory = 'sql' | 'document' | 'kv' | 'other';

export function syncCategory(dbType: string): SyncCategory {
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

/** Normalize a database type id to its sync dialect family (Transfer UI only). */
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

const pairingCache = new Map<string, SyncPairingResult>();

function pairingCacheKey(sourceType: string, targetType: string): string {
  return `${sourceType}\0${targetType}`;
}

function viewToResult(view: {
  path: string;
  supported: boolean;
  family?: string | null;
  reason?: string | null;
}): SyncPairingResult {
  return {
    path: view.path as SyncPath,
    supported: view.supported,
    family: view.family ?? undefined,
    reason: view.reason ?? undefined,
  };
}

/** Classify how a source/target database type pair should sync (backend IPC). */
export async function resolveSyncPairing(
  sourceType: string,
  targetType: string,
): Promise<SyncPairingResult> {
  const key = pairingCacheKey(sourceType, targetType);
  const cached = pairingCache.get(key);
  if (cached) return cached;

  const view = await syncCommands.classifyDataSyncPair(sourceType, targetType);
  const result = viewToResult(view);
  pairingCache.set(key, result);
  return result;
}

/** Whether target is selectable for sync given a source connection type. */
export async function isSyncTargetSupported(
  sourceType: string,
  targetType: string,
): Promise<boolean> {
  return (await resolveSyncPairing(sourceType, targetType)).supported;
}

/** Test helper — clear in-memory pairing cache between cases. */
export function clearSyncPairingCache(): void {
  pairingCache.clear();
}

/** React hook: target disabled map + active pair classification for Data Sync window. */
export function useSyncPairingState(
  sourceType: string | undefined,
  connections: { id: string; databaseType: string }[],
  targetId: string,
): {
  targetSupport: Record<string, boolean>;
  activePairing: SyncPairingResult | null;
} {
  const [targetSupport, setTargetSupport] = useState<Record<string, boolean>>({});
  const [activePairing, setActivePairing] = useState<SyncPairingResult | null>(null);

  useEffect(() => {
    if (!sourceType) {
      setTargetSupport({});
      return;
    }
    let cancelled = false;
    void (async () => {
      const entries = await Promise.all(
        connections.map(
          async (c) =>
            [c.id, await isSyncTargetSupported(sourceType, c.databaseType)] as const,
        ),
      );
      if (!cancelled) setTargetSupport(Object.fromEntries(entries));
    })();
    return () => {
      cancelled = true;
    };
  }, [sourceType, connections]);

  useEffect(() => {
    if (!sourceType || !targetId) {
      setActivePairing(null);
      return;
    }
    const tgt = connections.find((c) => c.id === targetId);
    if (!tgt) {
      setActivePairing(null);
      return;
    }
    let cancelled = false;
    void resolveSyncPairing(sourceType, tgt.databaseType).then((result) => {
      if (!cancelled) setActivePairing(result);
    });
    return () => {
      cancelled = true;
    };
  }, [sourceType, targetId, connections]);

  return { targetSupport, activePairing };
}
