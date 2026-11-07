import { useCallback, useEffect, useState } from 'react';
import { invokeDbSizes } from '../shared/redisInvoke';

/**
 * Per-database key counts for the sidebar's `(n)` suffix.
 *
 * Two sources feed one map (D-0 extraction, unchanged semantics):
 *  1. `db_sizes` — a single best-effort round trip covering all 16 dbs, refetched
 *     on session change and on explicit refresh;
 *  2. the active db's `dbSize` from `scan_keys` — free freshness for the db the
 *     user is looking at, at zero extra command cost.
 *
 * A failed `db_sizes` simply leaves the map empty: counts are decoration, never a
 * reason to block the tree.
 */

export interface DbKeyCounts {
  dbCounts: Record<number, number>;
  loadDbSizes: () => void;
}

export function useDbKeyCounts(
  dbSessionId: string,
  dbIndex: number,
  selectedDb: string | null,
  scannedDbSize: number,
): DbKeyCounts {
  const [dbCounts, setDbCounts] = useState<Record<number, number>>({});

  const loadDbSizes = useCallback(() => {
    void invokeDbSizes(dbSessionId)
      .then((sizes) => {
        const map: Record<number, number> = {};
        for (const s of sizes) map[s.db] = s.keys;
        setDbCounts(map);
      })
      .catch(() => {
        /* counts are best-effort enrichment */
      });
  }, [dbSessionId]);

  useEffect(() => {
    loadDbSizes();
  }, [loadDbSizes]);

  useEffect(() => {
    if (!selectedDb) return;
    setDbCounts((prev) =>
      prev[dbIndex] === scannedDbSize ? prev : { ...prev, [dbIndex]: scannedDbSize },
    );
  }, [selectedDb, dbIndex, scannedDbSize]);

  return { dbCounts, loadDbSizes };
}
