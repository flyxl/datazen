/**
 * Relation identity and the canonical cache key.
 *
 * This is the shared vocabulary of relation metadata: the SQL editor and the
 * Visual Query Builder both name relations with {@link QualifiedRelationId} and
 * both key the metadata cache with {@link buildEditorRelationKey}. Keeping the
 * derivation in one place is what lets either side read what the other loaded —
 * a second implementation would fold identifiers differently and silently split
 * one physical relation into two cache entries.
 */

import { foldIdentifierForDialect, resolveSqlDialectProfile } from '../sqlDialects/dialectProfile';
import { quoteIdentifierForDialect } from '../sqlDialects/identifierQuoting';

export type SqlIdentifierSegment = {
  name: string;
  quoted: boolean;
};

/** Database / schema / relation path without alias. */
export type QualifiedRelationId = {
  namespacePath: readonly SqlIdentifierSegment[];
  name: SqlIdentifierSegment;
};

/** How a relation was written in the SQL: a real table/view, or a query-local one. */
export type SqlRelationSourceKind = 'table' | 'view' | 'cte' | 'subquery';

/** Fold one segment: quoted segments keep their case, unquoted ones are folded. */
export function foldSegment(segment: SqlIdentifierSegment, dialectId: string): string {
  return segment.quoted ? segment.name : foldIdentifierForDialect(segment.name, dialectId);
}

/**
 * Normalized cross-namespace identity key (no dbSessionId).
 *
 * `public.users` and `PUBLIC.users` fold to the same key under PG, while
 * `"Users"` stays case-preserved. Segments keep their quoted state but are
 * joined into a single dotted string so the same physical relation always maps
 * to one key.
 */
export function relationIdentityKey(identity: QualifiedRelationId, dialectId: string): string {
  return [...identity.namespacePath, identity.name]
    .map((segment) => foldSegment(segment, dialectId))
    .join('.');
}

/**
 * Relation key = `dbSessionId::<normalized identity>`. Every loaded relation's
 * snapshot is keyed by this; distinct relations sharing a name across
 * namespaces never collide because the namespace path is part of it.
 */
export function buildEditorRelationKey(
  dbSessionId: string,
  identity: QualifiedRelationId,
  dialectId: string,
): string {
  return `${dbSessionId}::${relationIdentityKey(identity, dialectId)}`;
}

/**
 * Build the driver-accepted qualified identifier text for a relation.
 *
 * An empty namespacePath yields a bare name (the driver resolves it against the
 * session's current schema). Otherwise the namespace is qualified, preserving
 * each segment's quote style. Drivers that cannot resolve qualified targets must
 * not be asked for cross-namespace relations — callers gate on `allowQualified`.
 */
export function qualifiedNameText(identity: QualifiedRelationId, dialectId: string): string {
  const profile = resolveSqlDialectProfile(dialectId);
  return [...identity.namespacePath, identity.name]
    .map((segment) =>
      segment.quoted ? quoteIdentifierForDialect(segment.name, profile) : segment.name,
    )
    .join('.');
}

/** The (unqualified, normalized) relation name — e.g. `users` for `public.users`. */
export function relationBaseName(identity: QualifiedRelationId, dialectId: string): string {
  return foldSegment(identity.name, dialectId);
}

/** True when the relation carries an explicit namespace (not just the current schema). */
export function hasNamespace(identity: QualifiedRelationId): boolean {
  return identity.namespacePath.length > 0;
}
