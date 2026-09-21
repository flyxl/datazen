import { useEffect, useState } from 'react';
import { driverCommands } from '../../../commands/driver';

type DbCountsMap = Record<number, number>;

const cache = new Map<string, Promise<DbCountsMap>>();

function fetchDbCounts(dbSessionId: string, command: string): Promise<DbCountsMap> {
  let pending = cache.get(dbSessionId);
  if (!pending) {
    pending = driverCommands
      .execute({ dbSessionId, command, input: {} })
      .then((res) => {
        const rows = (Array.isArray(res.data) ? res.data : []) as Array<{
          db: number | string;
          keys: number;
        }>;
        const map: DbCountsMap = {};
        for (const r of rows) map[Number(r.db)] = Number(r.keys);
        return map;
      })
      .catch((e) => {
        cache.delete(dbSessionId);
        throw e;
      });
    cache.set(dbSessionId, pending);
  }
  return pending;
}

/** Drop cached counts for a session so the next read refetches (e.g. after flush). */
export function invalidateKvDbCounts(dbSessionId: string): void {
  cache.delete(dbSessionId);
}

/**
 * Lazily fetch per-database key counts for a KV connection via its driver's
 * `dbCountsCommand`. Shared/deduped per session through a module cache.
 */
export function useKvDbCounts(
  dbSessionId: string | undefined,
  command: string | undefined,
): DbCountsMap {
  const [counts, setCounts] = useState<DbCountsMap>({});
  useEffect(() => {
    if (!dbSessionId || !command) return;
    let alive = true;
    fetchDbCounts(dbSessionId, command)
      .then((map) => {
        if (alive) setCounts(map);
      })
      .catch(() => {
        /* counts are best-effort; ignore failures and render without them */
      });
    return () => {
      alive = false;
    };
  }, [dbSessionId, command]);
  return counts;
}
