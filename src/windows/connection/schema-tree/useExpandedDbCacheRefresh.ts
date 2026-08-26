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
  /** Live database session id, or empty while not connected. */
  dbSessionId?: string | null;
}

export interface ExpandedDbCacheRefreshOptions {
  /**
   * Active connections keyed by persistent config id — the same map the
   * navigator tree already maintains.
   */
  activeConnections: Record<string, ConnectionRef | undefined>;
  /**
   * Expanded db keys in `"<connectionId>::<dbName>"` form (persistent
   * connection ids). When a connection's fingerprint changes, every expanded
   * db of that connection is reloaded.
   */
  expandedDbs: ReadonlySet<string>;
  /**
   * Expanded object-category keys in
   * `"<connectionId>::<dbName>[::<schema>]::<catId>"` form. When a
   * connection's fingerprint changes, every expanded category of that
   * connection is reloaded (F1-BUG-005 fix).
   */
  expandedCats: ReadonlySet<string>;
  /** Load tables for one database. Must be session-neutral (no useDatabase). */
  loadTablesForDb: (dbSessionId: string, dbName: string) => Promise<void>;
  /**
   * Reload one object category into the caller's cache. `catId` is the trailing
   * segment of `catKey`. Must be session-neutral (no useDatabase).
   */
  loadObjectsForCat: (dbSessionId: string, catKey: string, catId: string) => Promise<void>;
  /**
   * Drop every cached entry belonging to the given runtime db session id.
   * `connectionId` is the persistent config id the session belongs to — table
   * caches are keyed by session id while object-category caches are keyed by
   * connection id, so invalidation needs both.
   */
  clearCaches: (dbSessionId: string, connectionId?: string) => void;
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
 *
 * Ordering / cancellation semantics (F1-BUG-005 fix): for every detected
 * fingerprint change this hook performs ONE synchronous
 * invalidate-then-schedule pass — `clearCaches` runs first, and the table +
 * object-category reloads for that connection are scheduled in the same
 * effect body with no `await` in between. A clear can therefore never leave
 * caches empty waiting for an unrelated reload to (maybe) win a race against
 * it: every wipe carries its own recovery wave. Explicit user refreshes may
 * still issue duplicate fetches afterwards; those writes are equally fresh,
 * so they can never resurrect stale content over the wave.
 */
export function useExpandedDbCacheRefresh({
  activeConnections,
  expandedDbs,
  expandedCats,
  loadTablesForDb,
  loadObjectsForCat,
  clearCaches,
}: ExpandedDbCacheRefreshOptions): void {
  // Stable snapshot of the latest callbacks without retriggering the effect.
  const handlersRef = useRef({ loadTablesForDb, loadObjectsForCat, clearCaches });
  handlersRef.current = { loadTablesForDb, loadObjectsForCat, clearCaches };

  const prevFpRef = useRef<Map<string, string>>(new Map());
  const schemas = useSchemaStore((s) => s.schemas);

  useEffect(() => {
    const nextFp = new Map<string, string>();
    for (const entry of Object.values(activeConnections)) {
      if (!entry?.dbSessionId) continue;
      const sd = schemas.get(entry.dbSessionId);
      if (sd) nextFp.set(entry.dbSessionId, schemaFingerprint(sd.databases, sd.schemaEpoch));
    }

    const prev = prevFpRef.current;
    prevFpRef.current = nextFp;
    if (prev.size === 0) return;

    for (const [dbSessionId, fp] of nextFp) {
      if (prev.get(dbSessionId) === fp) continue;

      // Map back to the persistent connection id used in expanded-db /
      // expanded-category keys.
      const connectionId = Object.entries(activeConnections).find(
        ([, e]) => e?.dbSessionId === dbSessionId,
      )?.[0];
      if (!connectionId) continue;

      // Invalidate first, then immediately schedule the recovery reloads —
      // synchronously within this effect body (see docstring above).
      handlersRef.current.clearCaches(dbSessionId, connectionId);

      const prefix = `${connectionId}::`;
      for (const dbKey of expandedDbs) {
        if (!dbKey.startsWith(prefix)) continue;
        void handlersRef.current.loadTablesForDb(dbSessionId, dbKey.slice(prefix.length));
      }
      for (const catKey of expandedCats) {
        if (!catKey.startsWith(prefix)) continue;
        const catId = catKey.split('::').pop();
        if (!catId || catId === 'tables' || catId === 'views') continue;
        void handlersRef.current.loadObjectsForCat(dbSessionId, catKey, catId);
      }
    }
    // Re-run only when the store surface or the expanded sets change; the
    // fingerprint comparison decides whether anything actually reloads.
  }, [activeConnections, schemas, expandedDbs, expandedCats]);
}
