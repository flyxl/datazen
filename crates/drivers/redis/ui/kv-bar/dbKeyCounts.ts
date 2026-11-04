/**
 * Per-database key count for the KV slots, read through `db_sizes`.
 *
 * `KvSlotState` (contract F-1) deliberately carries **no** key count: every
 * getter returns a scalar the workbench publishes, and the tree's own `DBSIZE`
 * is not one of them (`getLoadedCount()` is the *loaded* subset, a different
 * fact). So the status bar reads the driver command the rest of the driver
 * already uses for counts.
 *
 * **The in-flight merge below is not optional.** `db_sizes` is `SELECT` +
 * `DBSIZE` per database, i.e. 32 round trips on a 16-db server, and a caller
 * that mounts twice for one panel would double that on every db switch. The
 * merge is keyed **weakly by the panel relay** because that object *is* the
 * panel's identity for these slots (`useKvWorkspaceSlots` hands the same
 * `getKvSlotState(panelId)` to every slot, and the host prunes it when the
 * panel closes). Values are unsettled promises only, deleted the moment they
 * settle: this is an in-flight merge, never a cache — a finished read must not
 * be handed to a slot that mounts later, or a reopened panel would paint a
 * stale count (contract F-2 rejected driver-side caches for this reason).
 */
import { useEffect, useState } from 'react';
import type { KvSlotState } from '@datazen/driver-sdk';
import {
  invokeDbSizes,
  redisCommandInvoke,
  type DbSize,
  type RedisInvokeFn,
} from '../shared/redisInvoke';

/** In-flight `db_sizes` per panel relay, keyed by session as well. */
const openReads = new WeakMap<KvSlotState, Map<string, Promise<DbSize[]>>>();

/**
 * The `db_sizes` reply for `dbSessionId`, joining a request already in flight for
 * the same panel + session instead of issuing a second one.
 *
 * The invoker is part of the identity: a test and a component may pass different
 * seams, and merging across seams would hand one of them the other's stub.
 */
function sharedDbSizes(
  scope: KvSlotState,
  dbSessionId: string,
  invoke: RedisInvokeFn,
): Promise<DbSize[]> {
  let bySession = openReads.get(scope);
  if (!bySession) {
    bySession = new Map();
    openReads.set(scope, bySession);
  }
  const id = `${dbSessionId}\u0000${invoke === redisCommandInvoke ? 'default' : 'seam'}`;
  const joined = bySession.get(id);
  if (joined) return joined;
  const map = bySession;
  const flight = invokeDbSizes(dbSessionId, invoke).finally(() => {
    map.delete(id);
  });
  map.set(id, flight);
  return flight;
}

export interface DbKeyCount {
  /**
   * Key count of `dbIndex` in the panel's session; `null` while unknown — which
   * includes "the read failed" and "this db is not in the reply". Renderers must
   * treat `null` as *not rendered*, never as `0` (PRD §3.4 「不渲染」).
   */
  count: number | null;
}

/**
 * Key count of one database, for a KV slot.
 *
 * `null` on every unhappy path — a refused `DBSIZE` (ACL), a server that omits
 * the db, a malformed reply: counts are decoration, never a reason to fail a
 * slot or to print a zero the server never reported.
 *
 * The reply is tagged with the session it was issued for, so a session switch
 * cannot paint the previous connection's counts.
 */
export function useDbKeyCount(
  scope: KvSlotState,
  dbSessionId: string,
  dbIndex: number | undefined,
): DbKeyCount {
  const [entry, setEntry] = useState<{ session: string; count: number | null } | null>(null);

  useEffect(() => {
    if (!dbSessionId || dbIndex === undefined) {
      setEntry(null);
      return;
    }
    let live = true;
    void sharedDbSizes(scope, dbSessionId, redisCommandInvoke).then(
      (sizes) => {
        if (!live) return;
        const row = Array.isArray(sizes) ? sizes.find((size) => size.db === dbIndex) : undefined;
        setEntry({ session: dbSessionId, count: row ? row.keys : null });
      },
      () => {
        if (!live) return;
        setEntry({ session: dbSessionId, count: null });
      },
    );
    return () => {
      live = false;
    };
  }, [scope, dbSessionId, dbIndex]);

  // Derived rather than stored, so switching db/session never shows the previous
  // one's number for a frame (the same render-time invalidation the key read uses).
  if (!entry || entry.session !== dbSessionId) return { count: null };
  return { count: entry.count };
}
