import { useSyncExternalStore } from 'react';
import { useSchemaStore } from './schemaStore';
import {
  editorRelationKey,
  metadataCache,
  type MetadataCache,
} from '../components/sql-editor/metadata/metadataCache';
import type {
  EditorMetadataContext,
  EditorMetadataSnapshot,
  EditorRelationKey,
  EditorRelationMetadata,
  EditorRelationRequest,
} from '../components/sql-editor/metadata/types';
import type {
  QualifiedRelationId,
  SqlRelationBinding,
} from '../components/sql-editor/semantic/types';
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

/** Map semantic relation bindings to editor metadata requests (drops CTE/subquery). */
export function bindingToRelationRequests(
  bindings: readonly SqlRelationBinding[],
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
