import { useEffect, useRef } from 'react';
import { useSchemaStore } from '../../../stores/schemaStore';

/**
 * Fingerprint of a connection's database-list + schema-epoch. A change means
 * the connection's schema surface genuinely changed (databases added/removed,
 * or an admin op bumped the epoch) and per-database caches must be refreshed.
 */
function schemaFingerprint(databases: readonly string[] | undefined, epoch: number): string {
  return `${(databases ?? []).join('\0')}|${epoch}`;
}

interface ConnectionRef {
  /** Runtime connection id, or null/undefined while not connected. */
  connectionId?: string | null;
}

export interface ExpandedDbCacheRefreshOptions {
  /**
   * Active connections keyed by persistent config id — the same map the
   * navigator tree already maintains.
   */
  activeConnections: Record<string, ConnectionRef | undefined>;
  /**
   * Expanded db keys in `"<configId>::<dbName>"` form. When a connection's
   * fingerprint changes, every expanded db of that connection is reloaded.
   */
  expandedDbs: ReadonlySet<string>;
  /** Load tables for one database. Must be session-neutral (no useDatabase). */
  loadTablesForDb: (connectionId: string, dbName: string) => Promise<void>;
  /** Drop every cached entry belonging to the given runtime connection id. */
  clearCaches: (connectionId: string) => void;
}

/**
 * Watch the schema store for genuine schema-surface changes per connection
 * and refresh the caller's per-database caches for exactly those connections.
 *
 * Root-cause fix for "sidebar refresh flips my SQL session": the previous
 * inline implementations reloaded each expanded database through
 * `useDatabase` + `getTables`, so the LAST reloaded database silently became
 * the session's active database. Drivers now expose session-neutral table
 * reads, so this hook never touches `useDatabase`.
 */
export function useExpandedDbCacheRefresh({
  activeConnections,
  expandedDbs,
  loadTablesForDb,
  clearCaches,
}: ExpandedDbCacheRefreshOptions): void {
  // Stable snapshot of the latest callbacks without retriggering the effect.
  const handlersRef = useRef({ loadTablesForDb, clearCaches });
  handlersRef.current = { loadTablesForDb, clearCaches };

  const prevFpRef = useRef<Map<string, string>>(new Map());
  const schemas = useSchemaStore((s) => s.schemas);

  useEffect(() => {
    const nextFp = new Map<string, string>();
    for (const entry of Object.values(activeConnections)) {
      if (!entry?.connectionId) continue;
      const sd = schemas.get(entry.connectionId);
      if (sd) nextFp.set(entry.connectionId, schemaFingerprint(sd.databases, sd.schemaEpoch));
    }

    const prev = prevFpRef.current;
    prevFpRef.current = nextFp;
    if (prev.size === 0) return;

    for (const [connId, fp] of nextFp) {
      if (prev.get(connId) === fp) continue;

      handlersRef.current.clearCaches(connId);

      const configId = Object.entries(activeConnections).find(
        ([, e]) => e?.connectionId === connId,
      )?.[0];
      if (!configId) continue;

      const prefix = `${configId}::`;
      for (const dbKey of expandedDbs) {
        if (!dbKey.startsWith(prefix)) continue;
        void handlersRef.current.loadTablesForDb(connId, dbKey.slice(prefix.length));
      }
    }
    // Re-run only when the store surface or the expanded set changes; the
    // fingerprint comparison decides whether anything actually reloads.
  }, [activeConnections, schemas, expandedDbs]);
}
