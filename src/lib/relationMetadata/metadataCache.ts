/**
 * Session-scoped relation metadata cache.
 *
 * Process-wide singleton shared by the SQL editor (completion, hover, inlay,
 * navigation) and the Visual Query Builder (auto-JOIN detection). It lives below
 * both so that neither depends on the other, and it subscribes to schema
 * invalidation so a DDL statement refreshes every reader at once.
 */

import type { TableSchema } from '../../types';
import { getCachedTableSchema, subscribeSchemaInvalidation } from '../schemaCache';
import {
  buildEditorRelationKey,
  qualifiedNameText,
  relationBaseName,
  relationIdentityKey,
} from './identity';
import type {
  EditorMetadataContext,
  EditorMetadataSnapshot,
  EditorRelationKey,
  EditorRelationKind,
  EditorRelationMetadata,
  EditorRelationRequest,
} from './types';
import type { QualifiedRelationId } from './identity';

/** Default short TTL for a failed load — UI degrades without re-request storms. */
const DEFAULT_ERROR_TTL_MS = 10_000;
const DEFAULT_DEBOUNCE_MS = 40;

/**
 * A queued/loading request. `kind` is already resolved to a loadable relation
 * kind (cte/subquery filtered out).
 */
interface PendingRequest {
  identity: QualifiedRelationId;
  kind: EditorRelationKind;
}

interface SessionState {
  dbSessionId: string;
  dialectId: string;
  database: string;
  schema?: string;
  epoch: number;
  relations: Map<EditorRelationKey, EditorRelationMetadata>;
  /** Short-lived failure markers so we don't hammer a broken relation. */
  errors: Map<EditorRelationKey, { at: number }>;
  pending: Map<EditorRelationKey, PendingRequest>;
  inflight: Map<EditorRelationKey, PendingRequest>;
  cachedSnapshot?: EditorMetadataSnapshot;
  timer?: ReturnType<typeof setTimeout>;
}

export interface MetadataCacheDeps {
  /** IPC-layer schema fetch; defaults to `getCachedTableSchema` (had already inflight/TTL/freeze). */
  loadTableSchema?: (
    dbSessionId: string,
    table: string,
    database: string,
    schema?: string,
  ) => Promise<TableSchema>;
  debounceMs?: number;
  errorTtlMs?: number;
  /** If false, cross-namespace relations are not requested (driver can't resolve qualified). */
  allowQualified?: boolean;
  now?: () => number;
  /**
   * External invalidation source (e.g. `subscribeSchemaInvalidation`). When the
   * schema/DDL cache is invalidated (schema refresh, DDL mutation, session
   * close), the affected editor metadata is dropped too. No-op when omitted.
   */
  subscribeInvalidation?: (
    listener: (dbSessionId: string, tableName?: string) => void,
  ) => () => void;
}

export interface MetadataCache {
  /** Queue relation metadata requests (debounced, deduped). Callers may not await. */
  ensureRelations(
    dbSessionId: string,
    requests: readonly EditorRelationRequest[],
    ctx: EditorMetadataContext,
  ): void;
  /** Force-flush queued requests immediately; awaits all in-flight loads. */
  flushNow(): Promise<void>;
  /** Synchronous immutable snapshot. Referential-stable until the epoch bumps. */
  getSnapshot(dbSessionId: string): EditorMetadataSnapshot;
  getRelation(dbSessionId: string, key: EditorRelationKey): EditorRelationMetadata | undefined;
  /** Subscribe to snapshot changes; returns an unsubscribe. */
  subscribe(listener: () => void): () => void;
  invalidateRelation(dbSessionId: string, identity: QualifiedRelationId, dialectId: string): void;
  invalidateSession(dbSessionId: string): void;
  invalidateAll(): void;
  /** Reset database/schema/dialect context and drop the session's loaded relations. */
  switchContext(dbSessionId: string, ctx: EditorMetadataContext): void;
}

function deepFreeze<T>(value: T): T {
  if (value == null || typeof value !== 'object') return value;
  const seen = new WeakSet<object>();
  const walk = (v: unknown) => {
    if (v == null || typeof v !== 'object' || seen.has(v as object)) return;
    seen.add(v as object);
    for (const key of Object.keys(v as object)) {
      walk((v as Record<string, unknown>)[key]);
    }
    Object.freeze(v);
  };
  walk(value);
  return value;
}

/** Immutable copy of the live relation map (never freezes the working map). */
function frozenRelationMap(
  relations: Map<EditorRelationKey, EditorRelationMetadata>,
): ReadonlyMap<EditorRelationKey, EditorRelationMetadata> {
  const copy = new Map(relations);
  return Object.freeze(copy);
}

/** Build immutable relation metadata from a fetched (already frozen) schema. */
export function schemaToMetadata(
  key: EditorRelationKey,
  identity: QualifiedRelationId,
  kind: EditorRelationKind,
  schema: TableSchema,
  loadedAt: number,
): EditorRelationMetadata {
  const comment =
    (schema as Partial<TableSchema> & { comment?: string | null }).comment ?? undefined;
  return deepFreeze({
    key,
    identity,
    kind,
    columns: schema.columns ?? [],
    primaryKey: schema.primaryKeys ?? [],
    indexes: schema.indexes ?? [],
    foreignKeys: schema.foreignKeys ?? [],
    comment,
    loadedAt,
  });
}

const EMPTY_RELATIONS: ReadonlyMap<EditorRelationKey, EditorRelationMetadata> = new Map();

function emptySnapshot(dbSessionId: string): EditorMetadataSnapshot {
  return Object.freeze({
    dbSessionId,
    database: '',
    epoch: 0,
    relations: EMPTY_RELATIONS,
  });
}

function toLoadableKind(request: EditorRelationRequest): EditorRelationKind | null {
  switch (request.kind) {
    case 'view':
      return 'view';
    case 'cte':
    case 'subquery':
      return null;
    case 'table':
      return 'table';
    default:
      return 'table';
  }
}

/** True when an external invalidation name matches a loaded relation. */
function relationMatchesName(
  dialectId: string,
  identity: QualifiedRelationId,
  tableName: string,
): boolean {
  return (
    qualifiedNameText(identity, dialectId) === tableName ||
    relationBaseName(identity, dialectId) === tableName
  );
}

export function createMetadataCache(deps: MetadataCacheDeps = {}): MetadataCache {
  const loadTableSchema = deps.loadTableSchema ?? getCachedTableSchema;
  const debounceMs = deps.debounceMs ?? DEFAULT_DEBOUNCE_MS;
  const errorTtlMs = deps.errorTtlMs ?? DEFAULT_ERROR_TTL_MS;
  const allowQualified = deps.allowQualified ?? true;
  const now = deps.now ?? Date.now;

  const sessions = new Map<string, SessionState>();
  const emptySnapshots = new Map<string, EditorMetadataSnapshot>();
  const listeners = new Set<() => void>();

  function notify(): void {
    for (const listener of [...listeners]) listener();
  }

  function buildSnapshot(session: SessionState): EditorMetadataSnapshot {
    return Object.freeze({
      dbSessionId: session.dbSessionId,
      database: session.database,
      schema: session.schema,
      epoch: session.epoch,
      relations: frozenRelationMap(session.relations),
    });
  }

  /** Bump the epoch, rebuild the cached snapshot, and notify subscribers. */
  function refreshSnapshot(session: SessionState): void {
    session.epoch += 1;
    session.cachedSnapshot = buildSnapshot(session);
    notify();
  }

  function getOrCreateSession(dbSessionId: string, database?: string): SessionState {
    let session = sessions.get(dbSessionId);
    if (session == null) {
      session = {
        dbSessionId,
        dialectId: 'standard',
        // Empty until the first switchContext/ensureRelations pins the tab's
        // database; loadOne guards and re-queues instead of querying empty.
        database: database ?? '',
        epoch: 0,
        relations: new Map(),
        errors: new Map(),
        pending: new Map(),
        inflight: new Map(),
      };
      session.cachedSnapshot = buildSnapshot(session);
      sessions.set(dbSessionId, session);
    }
    return session;
  }

  function cancelTimer(session: SessionState): void {
    if (session.timer != null) {
      clearTimeout(session.timer);
      session.timer = undefined;
    }
  }

  function scheduleFlush(session: SessionState): void {
    if (session.timer != null || session.pending.size === 0) return;
    session.timer = setTimeout(() => {
      session.timer = undefined;
      void flushSession(session);
    }, debounceMs);
  }

  async function flushSession(session: SessionState): Promise<void> {
    if (session.pending.size === 0) return;
    const batch = [...session.pending.values()];
    session.pending.clear();

    const jobs: Promise<void>[] = [];
    for (const request of batch) {
      const key = buildEditorRelationKey(session.dbSessionId, request.identity, session.dialectId);
      if (session.inflight.has(key)) continue;
      session.inflight.set(key, request);
      jobs.push(
        loadOne(session, key, request).finally(() => {
          if (session.inflight.get(key) === request) session.inflight.delete(key);
        }),
      );
    }
    await Promise.allSettled(jobs);
  }

  async function loadOne(
    session: SessionState,
    key: EditorRelationKey,
    request: PendingRequest,
  ): Promise<void> {
    // Database is mandatory: never query without the tab-bound database.
    // If the context hasn't arrived yet, re-queue for the next flush.
    if (!session.database) {
      session.pending.set(key, request);
      return;
    }
    const database = session.database;
    const schemaContext = session.schema;
    const dialectId = session.dialectId;
    const isCurrent = () =>
      sessions.get(session.dbSessionId) === session &&
      session.database === database &&
      session.schema === schemaContext &&
      session.dialectId === dialectId &&
      session.inflight.get(key) === request;
    const table = qualifiedNameText(request.identity, session.dialectId);
    try {
      // The session's schema is part of the relation's identity: omitting it
      // makes the host fall back to the connection default, which resolves a
      // relation that lives elsewhere as missing.
      const schema = await loadTableSchema(session.dbSessionId, table, database, schemaContext);
      if (!isCurrent()) return;
      const metadata = schemaToMetadata(key, request.identity, request.kind, schema, now());
      session.relations.set(key, metadata);
      session.errors.delete(key);
      refreshSnapshot(session);
    } catch {
      if (!isCurrent()) return;
      session.errors.set(key, { at: now() });
      // No epoch bump — the relation simply stays absent; UI degrades on its own.
    }
  }

  function requestKey(session: SessionState, request: EditorRelationRequest): EditorRelationKey {
    return buildEditorRelationKey(session.dbSessionId, request.identity, session.dialectId);
  }

  function enqueue(session: SessionState, request: EditorRelationRequest): void {
    const loadableKind = toLoadableKind(request);
    if (loadableKind == null) return;
    if (!allowQualified) {
      // Driver can't resolve a qualified target: only load bare current-schema names.
      if (request.identity.namespacePath.length > 0) return;
    }
    const key = requestKey(session, request);
    if (session.relations.has(key) || session.inflight.has(key) || session.pending.has(key)) return;
    const err = session.errors.get(key);
    if (err && now() - err.at < errorTtlMs) return;
    session.pending.set(key, { identity: request.identity, kind: loadableKind });
  }

  function ensureRelationsTarget(
    dbSessionId: string,
    requests: readonly EditorRelationRequest[],
    ctx: EditorMetadataContext,
  ): void {
    const session = getOrCreateSession(dbSessionId, ctx.database);
    const dialectId = ctx.dialectId || 'standard';
    if (
      session.database !== ctx.database ||
      session.schema !== ctx.schema ||
      session.dialectId !== dialectId
    ) {
      if (session.epoch === 0 && session.relations.size === 0 && session.inflight.size === 0) {
        session.database = ctx.database;
        session.schema = ctx.schema;
        session.dialectId = dialectId;
        session.cachedSnapshot = buildSnapshot(session);
      } else {
        resetContext(session, ctx);
      }
    }
    for (const request of requests) enqueue(session, request);
    scheduleFlush(session);
  }

  function relationMatchesNameTarget(dbSessionId: string, tableName?: string): void {
    const session = sessions.get(dbSessionId);
    if (session == null) return;
    if (tableName == null) {
      // Whole-session invalidation (schema refresh / session close).
      cancelTimer(session);
      sessions.delete(dbSessionId);
      emptySnapshots.delete(dbSessionId);
      notify();
      return;
    }
    for (const [key, meta] of [...session.relations]) {
      if (relationMatchesName(session.dialectId, meta.identity, tableName)) {
        session.relations.delete(key);
        session.errors.delete(key);
      }
    }
    for (const entries of [session.pending, session.inflight]) {
      for (const [key, request] of entries) {
        if (relationMatchesName(session.dialectId, request.identity, tableName)) {
          entries.delete(key);
          session.errors.delete(key);
        }
      }
    }
    // Refresh also when an in-flight-only relation was invalidated.
    refreshSnapshot(session);
  }

  function resetContext(session: SessionState, ctx: EditorMetadataContext): void {
    cancelTimer(session);
    session.database = ctx.database;
    session.schema = ctx.schema;
    session.dialectId = ctx.dialectId || 'standard';
    session.relations.clear();
    session.errors.clear();
    session.pending.clear();
    session.inflight.clear();
    refreshSnapshot(session);
  }

  if (deps.subscribeInvalidation) {
    deps.subscribeInvalidation((dbSessionId, tableName) =>
      relationMatchesNameTarget(dbSessionId, tableName),
    );
  }

  return {
    ensureRelations: ensureRelationsTarget,

    flushNow: async () => {
      for (const session of sessions.values()) cancelTimer(session);
      await Promise.allSettled([...sessions.values()].map((s) => flushSession(s)));
    },

    getSnapshot: (dbSessionId: string) => {
      const session = sessions.get(dbSessionId);
      if (session == null) {
        // Referentially stable empty snapshot for unknown sessions so that
        // `useSyncExternalStore` doesn't re-render on every read.
        let empty = emptySnapshots.get(dbSessionId);
        if (empty == null) {
          empty = emptySnapshot(dbSessionId);
          emptySnapshots.set(dbSessionId, empty);
        }
        return empty;
      }
      if (session.cachedSnapshot == null) session.cachedSnapshot = buildSnapshot(session);
      return session.cachedSnapshot;
    },

    getRelation: (dbSessionId: string, key: EditorRelationKey) => {
      return sessions.get(dbSessionId)?.relations.get(key);
    },

    subscribe: (listener: () => void) => {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },

    invalidateRelation: (dbSessionId: string, identity: QualifiedRelationId, dialectId: string) => {
      const session = sessions.get(dbSessionId);
      if (session == null) return;
      const key = buildEditorRelationKey(dbSessionId, identity, dialectId ?? session.dialectId);
      session.relations.delete(key);
      session.errors.delete(key);
      session.pending.delete(key);
      session.inflight.delete(key);
      refreshSnapshot(session);
    },

    invalidateSession: (dbSessionId: string) => {
      const session = sessions.get(dbSessionId);
      if (session == null) return;
      cancelTimer(session);
      sessions.delete(dbSessionId);
      emptySnapshots.delete(dbSessionId);
      notify();
    },

    invalidateAll: () => {
      for (const session of sessions.values()) cancelTimer(session);
      sessions.clear();
      emptySnapshots.clear();
      notify();
    },

    switchContext: (dbSessionId: string, ctx: EditorMetadataContext) => {
      const session = getOrCreateSession(dbSessionId, ctx.database);
      resetContext(session, ctx);
    },
  };
}

/**
 * Default process-wide editor metadata cache, wired to schema/DDL invalidations.
 *
 * KNOWN MULTI-PANE HAZARD — deliberately not fixed in the pane-layout wave.
 * This is a process-level singleton (one instance for the whole app), and its
 * sessions are keyed by `dbSessionId` alone: `switchContext()` overwrites the
 * `database` / `schema` / `dialectId` of that session and clears its relation
 * maps in place. Once one tab is split into several panes, two editors on the
 * same `dbSessionId` share a single context slot, so focusing pane B after
 * pane A changed its context silently invalidates A's cached snapshot and every
 * completion A triggers afterwards re-queries under B's context.
 *
 * The fix belongs to the next pane wave and is a real state-model change, not a
 * cosmetic one: either key the cache by `dbSessionId::paneId` (a pane-key
 * dimension, mirroring `src/stores/paneKeys.ts`) or make the cache reference
 * counted per pane and tear a session down when its last pane closes. Until then
 * only one pane per tab may exist, so this hazard is currently unreachable.
 * Only one pane per tab may exist today, so the cache is keyed by session.
 */
export const metadataCache = createMetadataCache({
  subscribeInvalidation: subscribeSchemaInvalidation,
});

/** Utility: normalize a request to its editor key (for callers that need it). */
export function editorRelationKey(
  dbSessionId: string,
  identity: QualifiedRelationId,
  dialectId: string,
): EditorRelationKey {
  return buildEditorRelationKey(dbSessionId, identity, dialectId);
}

export { relationIdentityKey };
