/**
 * Batched, isolation-checked column reads for the schema store.
 *
 * `MetadataBatcher` exists so that metadata reads never merge across the three
 * isolation dimensions (identity, configuration, target). This module is its
 * production home: every column read the schema store issues is queued here
 * first and grouped by the {@link metadataBatchBucket} of the relation's target
 * namespace, so one `readColumns` call can only ever carry relations that share
 * a connection config, a config revision, a runtime session *and* a namespace.
 *
 * What merging buys, concretely: completion, hover and the tree view routinely
 * ask for columns of relations in the same namespace within one tick. Before
 * batching each such ask produced its own round trip; now they coalesce into a
 * single read per namespace. Merging never crosses a bucket, so two sessions,
 * two config revisions or two namespaces still produce separate reads.
 *
 * Coalescing needs a window: {@link MetadataBatcher} is a trailing debounce, so
 * a zero window flushes every ask on its own and the batch collapses into a
 * pass-through. The default matches the editor relation-metadata cache, which
 * coalesces the same kind of burst with the same window.
 *
 * Callers still perform their own identity re-check before committing a result
 * — this module guarantees *which read* a request belongs to, not that a result
 * read a moment ago is still wanted.
 */

import { MetadataBatcher, metadataBatchBucket } from '../lib/metadata';
import { loadRelationColumns } from './schemaColumnLoader';
import {
  identityForSession,
  relationColumnsCacheKey,
  type SessionMetadataIdentity,
} from './schemaMetadataKeys';
import type { MetadataIdentityInput } from '../lib/metadata';
import type { RelationColumns, RelationRef } from '@datazen/driver-sdk';

/** One queued read plus everyone waiting on it. */
interface PendingRead {
  session: SessionMetadataIdentity;
  dbSessionId: string;
  /** Keyed by the A-layer relation key, so refs are unique within a read. */
  refs: Map<string, RelationRef>;
  waiters: ReadWaiter[];
}

interface ReadWaiter {
  /** Only the values for the relations this caller asked for. */
  wanted: Set<string>;
  resolve: (values: RelationColumns[]) => void;
  reject: (error: unknown) => void;
}

/** The transport, injectable so tests need neither IPC nor a driver. */
export type ColumnReadTransport = (
  session: SessionMetadataIdentity,
  dbSessionId: string,
  relations: readonly RelationRef[],
) => Promise<RelationColumns[]>;

export interface SchemaColumnBatcherOptions {
  /**
   * Coalescing window in ms. Defaults to {@link DEFAULT_BATCH_DEBOUNCE_MS}: a
   * read starts once the namespace has been quiet for the window, so a burst of
   * asks costs one round trip. Set it to 0 to read every ask immediately (and
   * give up merging) — e.g. in tests.
   */
  debounceMs?: number;
  read?: ColumnReadTransport;
}

/**
 * Coalescing window, matching the editor relation-metadata cache's
 * `DEFAULT_DEBOUNCE_MS`: long enough to collect one tick's asks, short enough
 * that a completion read stays imperceptible.
 */
export const DEFAULT_BATCH_DEBOUNCE_MS = 40;

export interface SchemaColumnBatcher {
  /** Queue `refs` and resolve with the columns read for exactly those refs. */
  ensure(
    session: SessionMetadataIdentity,
    dbSessionId: string,
    refs: readonly RelationRef[],
  ): Promise<RelationColumns[]>;
  /** Drop every queued read of `dbSessionId` (session close). */
  invalidateSession(dbSessionId: string): void;
  /** Number of queued reads (observability / tests). */
  readonly size: number;
}

/** The identity of one relation read, i.e. its isolation dimensions + target. */
function readIdentity(
  session: SessionMetadataIdentity,
  dbSessionId: string,
  ref: RelationRef,
): MetadataIdentityInput {
  return identityForSession({
    connectionId: session.connectionId,
    configRevision: session.metadataRevision,
    dbSessionId,
    target: {
      connectionId: session.connectionId as MetadataIdentityInput['connectionId'],
      namespace: {
        database: ref.database ?? null,
        catalog: null,
        schema: ref.schema ?? null,
        path: [],
      },
      object: { kind: 'relation', name: ref.name, signature: null },
    },
  });
}

export function createSchemaColumnBatcher(
  options: SchemaColumnBatcherOptions = {},
): SchemaColumnBatcher {
  const read: ColumnReadTransport = options.read ?? loadRelationColumns;
  const reads = new Map<string, PendingRead>();

  const batcher = new MetadataBatcher(
    async (bucket) => {
      const pending = reads.get(bucket);
      // Invalidated between enqueue and flush: there is nothing to read for.
      if (pending == null) return;
      reads.delete(bucket);
      const waiters = pending.waiters;
      try {
        const values = await read(pending.session, pending.dbSessionId, [...pending.refs.values()]);
        // Each caller receives only its own relations: a shared read must not
        // hand a caller columns it never asked for.
        for (const waiter of waiters) {
          waiter.resolve(
            values.filter((value) =>
              waiter.wanted.has(
                relationColumnsCacheKey(pending.session, pending.dbSessionId, value.ref),
              ),
            ),
          );
        }
      } catch (error) {
        for (const waiter of waiters) waiter.reject(error);
      }
    },
    { debounceMs: options.debounceMs ?? DEFAULT_BATCH_DEBOUNCE_MS },
  );

  return {
    ensure(session, dbSessionId, refs) {
      if (refs.length === 0) return Promise.resolve([]);
      // One read per target namespace. Relations in different namespaces are
      // different metadata targets and must never share a round trip.
      const byBucket = new Map<string, { ref: RelationRef; key: string; input: MetadataIdentityInput }[]>();
      for (const ref of refs) {
        const input = readIdentity(session, dbSessionId, ref);
        const bucket = metadataBatchBucket(input);
        const group = byBucket.get(bucket);
        const entry = { ref, key: relationColumnsCacheKey(session, dbSessionId, ref), input };
        if (group) group.push(entry);
        else byBucket.set(bucket, [entry]);
      }

      const promises: Promise<RelationColumns[]>[] = [];
      for (const [bucket, group] of byBucket) {
        let pending = reads.get(bucket);
        if (pending == null) {
          pending = { session, dbSessionId, refs: new Map(), waiters: [] };
          reads.set(bucket, pending);
        }
        promises.push(
          new Promise<RelationColumns[]>((resolve, reject) => {
            pending.waiters.push({
              wanted: new Set(group.map((entry) => entry.key)),
              resolve,
              reject,
            });
          }),
        );
        for (const entry of group) {
          pending.refs.set(entry.key, entry.ref);
          batcher.enqueue(entry.input, entry.key);
        }
      }
      // The bucket's own debounce timer starts the read; flushing here would
      // defeat the coalescing this module exists for.
      return promises.length === 1
        ? promises[0]
        : Promise.all(promises).then((groups) => groups.flat());
    },

    invalidateSession(dbSessionId) {
      const dropped = new Set<string>();
      for (const [bucket, pending] of [...reads.entries()]) {
        if (pending.dbSessionId !== dbSessionId) continue;
        dropped.add(bucket);
        reads.delete(bucket);
        // Settle, never hang: the session is gone, so no caller may still be
        // waiting on a read it can no longer commit.
        for (const waiter of pending.waiters) {
          waiter.reject(new Error(`metadata read dropped: session ${dbSessionId} closed`));
        }
      }
      batcher.invalidate((bucket) => dropped.has(bucket));
    },

    get size() {
      return batcher.size;
    },
  };
}

/** Process-wide batcher used by the schema store's column reads. */
export const schemaColumnBatcher = createSchemaColumnBatcher();

/**
 * Exit transition for a closed session.
 *
 * Called by the connection store when a runtime session goes away, so queued
 * reads are dropped instead of racing a `dbSessionId` that no longer exists.
 */
export function dropColumnReadsForSession(dbSessionId: string): void {
  schemaColumnBatcher.invalidateSession(dbSessionId);
}