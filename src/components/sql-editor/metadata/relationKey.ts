import { getDialectAdapter } from '../semantic/dialectAdapter';
import { foldSegment, relationKey } from '../semantic/quoteHelper';
import type { SqlDialectAdapter } from '../semantic/types';
import type { QualifiedRelationId } from '../semantic/types';
import type { EditorRelationKey } from './types';

/**
 * Normalized cross-namespace identity key (no dbSessionId).
 *
 * `public.users` and `PUBLIC.users` fold to the same key under PG, while
 * `"Users"` stays case-preserved. Segments keep their quoted state but are
 * joined into a single dotted string so the same physical relation always
 * maps to one key.
 */
export function relationIdentityKey(identity: QualifiedRelationId, dialectId: string): string {
  const adapter = getDialectAdapter(dialectId);
  return relationKey(identity, adapter);
}

/**
 * Editor relation key = `dbSessionId::<normalized identity>`. Every loaded
 * relation's snapshot is keyed by this; distinct relations sharing a name
 * across namespaces never collide because the namespace path is part of it.
 */
export function buildEditorRelationKey(
  dbSessionId: string,
  identity: QualifiedRelationId,
  dialectId: string,
): EditorRelationKey {
  return `${dbSessionId}::${relationIdentityKey(identity, dialectId)}`;
}

function segmentToText(segment: QualifiedRelationId['name'], adapter: SqlDialectAdapter): string {
  return segment.quoted ? adapter.quoteIdentifier(segment.name) : segment.name;
}

/**
 * Build the driver-accepted qualified identifier text for a relation.
 *
 * An empty namespacePath yields a bare name (the driver resolves it against the
 * session's current schema). Otherwise the namespace is qualified, preserving
 * each segment's quote style. Drivers that cannot resolve qualified targets
 * must not be asked for cross-namespace relations — callers gate on
 * `allowQualified`.
 */
export function qualifiedNameText(identity: QualifiedRelationId, dialectId: string): string {
  const adapter = getDialectAdapter(dialectId);
  return [...identity.namespacePath, identity.name].map((s) => segmentToText(s, adapter)).join('.');
}

/** The (unqualified, normalized) relation name — e.g. `users` for `public.users`. */
export function relationBaseName(identity: QualifiedRelationId, dialectId: string): string {
  return foldSegment(identity.name, getDialectAdapter(dialectId));
}

/** True when the relation carries an explicit namespace (not just the current schema). */
export function hasNamespace(identity: QualifiedRelationId): boolean {
  return identity.namespacePath.length > 0;
}
