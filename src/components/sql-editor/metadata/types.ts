import type { ColumnSchema, ForeignKeyInfo, IndexInfo } from '../../../types';
import type { QualifiedRelationId, SqlRelationSourceKind } from '../semantic/types';

/** A relation the editor can prefetch metadata for: a real table or view. */
export type EditorRelationKind = 'table' | 'view';

/**
 * Canonical, normalized relation key.
 *
 * Encodes `dbSessionId + namespacePath + relation name`, where every segment
 * keeps its quoted state and unquoted segments are folded by the active
 * dialect adapter. This lets the same physical relation always map to the
 * same key regardless of how the user spells it (`public.users`,
 * `PUBLIC.USERS`, `"public".users`), while keeping distinct relations that
 * share a name across schemas/namespaces separate.
 */
export type EditorRelationKey = string;

/** Immutable metadata snapshot entry for one loaded relation. */
export interface EditorRelationMetadata {
  key: EditorRelationKey;
  identity: QualifiedRelationId;
  kind: EditorRelationKind;
  /** Column detail list. Per-column `comment` stays optional. */
  columns: readonly ColumnSchema[];
  primaryKey: readonly string[];
  indexes: readonly IndexInfo[];
  foreignKeys: readonly ForeignKeyInfo[];
  /** Table-level comment. Optional: no data → hide the line, never fabricate. */
  comment?: string;
  loadedAt: number;
}

/**
 * Immutable snapshot the completion / hover / inlay callbacks read synchronously.
 *
 * All fields are read-only. `getSnapshot` returns the *same reference* until the
 * session epoch bumps, so `useSyncExternalStore` can depend on referential
 * equality between renders.
 */
export interface EditorMetadataSnapshot {
  dbSessionId: string;
  /** Stable database context the relations were resolved against (if any). */
  database?: string;
  /** Stable schema context the relations were resolved against (if any). */
  schema?: string;
  /** Monotonic generation counter; bumps on any relations/context change. */
  epoch: number;
  relations: ReadonlyMap<EditorRelationKey, EditorRelationMetadata>;
}

/** How the semantic model reported a relation. */
export type EditorRelationSourceKind = SqlRelationSourceKind;

/** A queued metadata request: which relation to load, and its kind hint. */
export interface EditorRelationRequest {
  identity: QualifiedRelationId;
  /** `'table' | 'view'` from the semantic binding; `'cte' | 'subquery'` are skipped. */
  kind?: EditorRelationSourceKind;
}

/** Context needed to resolve/normalize a relation for a session. */
export interface EditorMetadataContext {
  database?: string;
  schema?: string;
  /** Dialect id (e.g. `postgresql`, `mysql`) — drives identifier folding/quoting. */
  dialectId: string;
}
