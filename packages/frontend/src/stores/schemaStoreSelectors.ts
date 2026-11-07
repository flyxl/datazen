import { useSyncExternalStore } from 'react';
import { useSchemaStore } from './schemaStore';
import {
  editorRelationKey,
  metadataCache,
  type MetadataCache,
} from '../lib/relationMetadata/metadataCache';
import type {
  EditorMetadataContext,
  EditorMetadataSnapshot,
  EditorRelationKey,
  EditorRelationMetadata,
  EditorRelationRequest,
  QualifiedRelationId,
  SqlRelationSourceKind,
} from '../lib/relationMetadata/types';

import { DB_REGISTRY } from '../lib/databaseTypes';
import type { DatabaseType } from '../types';

/** Subset of the cache API the selectors depend on (injectable for tests). */
export type EditorMetadataCacheLike = Pick<
  MetadataCache,
  | 'ensureRelations'
  | 'flushNow'
  | 'getSnapshot'
  | 'getRelation'
  | 'subscribe'
  | 'invalidateRelation'
  | 'invalidateSession'
  | 'invalidateAll'
  | 'switchContext'
>;

/** Synchronously read the immutable snapshot for a session (no IPC). */
export function readMetadataSnapshot(
  dbSessionId: string,
  cache: EditorMetadataCacheLike = metadataCache,
): EditorMetadataSnapshot {
  return cache.getSnapshot(dbSessionId);
}

/** Synchronously read a single relation's metadata (no IPC). */
export function getMetadataRelation(
  dbSessionId: string,
  key: EditorRelationKey,
  cache: EditorMetadataCacheLike = metadataCache,
): EditorRelationMetadata | undefined {
  return cache.getRelation(dbSessionId, key);
}

/** Queue relation metadata loads (debounced/deduped in the cache). */
export function ensureMetadataRelations(
  dbSessionId: string,
  requests: readonly EditorRelationRequest[],
  ctx: EditorMetadataContext,
  cache: EditorMetadataCacheLike = metadataCache,
): void {
  cache.ensureRelations(dbSessionId, requests, ctx);
}

/**
 * A relation the SQL semantic model reported.
 *
 * Declared structurally rather than imported from the editor's semantic layer:
 * the store must not depend on a peer that sits above it. `SqlRelationBinding`
 * satisfies this shape, so editor callers need no cast.
 */
export interface RelationBindingLike {
  relation: QualifiedRelationId;
  sourceKind?: SqlRelationSourceKind;
}

/** Map semantic relation bindings to metadata requests (drops CTE/subquery). */
export function bindingToRelationRequests(
  bindings: readonly RelationBindingLike[],
): EditorRelationRequest[] {
  const requests: EditorRelationRequest[] = [];
  for (const binding of bindings) {
    if (binding.sourceKind === 'cte' || binding.sourceKind === 'subquery') continue;
    requests.push({ identity: binding.relation, kind: binding.sourceKind });
  }
  return requests;
}

/** Normalize a relation to its editor cache key. */
export function relationCacheKey(
  dbSessionId: string,
  identity: QualifiedRelationId,
  dialectId: string,
): EditorRelationKey {
  return editorRelationKey(dbSessionId, identity, dialectId);
}

/** Map a DatabaseType to the semantic dialect id used for identifier folding. */
export function resolveEditorDialectId(databaseType?: string | null): string {
  if (!databaseType) return 'standard';
  const family = DB_REGISTRY[databaseType as DatabaseType]?.sqlDialect;
  return family ?? databaseType;
}

/**
 * Map a relation to its metadata-cache key.
 *
 * Re-exported so consumers (notably the Visual Query Builder) can key a lookup
 * without importing the metadata implementation directly — the store is the
 * shared layer both the editor and the builder sit on.
 */
export function relationLookupKey(
  dbSessionId: string,
  identity: QualifiedRelationId,
  dialectId: string,
): EditorRelationKey {
  return editorRelationKey(dbSessionId, identity, dialectId);
}

/** React hook: subscribe to the metadata snapshot for a session (or `null` → empty). */
export function useMetadataSnapshot(
  dbSessionId: string | null,
  cache: EditorMetadataCacheLike = metadataCache,
): EditorMetadataSnapshot {
  const key = dbSessionId ?? '__inactive__';
  return useSyncExternalStore(cache.subscribe, () => cache.getSnapshot(key));
}

/**
 * React hook: metadata snapshot for the store's active DB session.
 * Bridges the schemaStore's `activeDbSessionId` to the editor metadata cache
 * without modifying schemaStore.ts.
 */
export function useActiveSessionMetadataSnapshot(): EditorMetadataSnapshot {
  const dbSessionId = useSchemaStore((s) => s.activeDbSessionId);
  return useMetadataSnapshot(dbSessionId);
}
