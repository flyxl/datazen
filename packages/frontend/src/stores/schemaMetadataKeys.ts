/**
 * Store-side metadata identity keys.
 *
 * The schema store addresses its per-relation metadata cache through the
 * identity/config/target keys defined in `src/lib/metadata` — never through
 * ad-hoc strings — so two reads can only share a cache entry when every
 * isolation dimension matches: the persistent connection config, that config's
 * session-local revision, the runtime session, and the target relation.
 */

import { metadataCacheKey, type MetadataIdentityInput } from '../lib/metadata';
import type { ExecutionTarget, NamespaceTarget } from '@datazen/backend-client';
import type { RelationRef } from '@datazen/driver-sdk';

/** Build the identity input that isolates one metadata request. */
export function identityForSession(input: {
  connectionId: string;
  configRevision: number;
  dbSessionId: string;
  target: ExecutionTarget | NamespaceTarget;
}): MetadataIdentityInput {
  return {
    connectionId: input.connectionId as MetadataIdentityInput['connectionId'],
    configRevision: input.configRevision as MetadataIdentityInput['configRevision'],
    dbSessionId: input.dbSessionId as MetadataIdentityInput['dbSessionId'],
    target: input.target,
  };
}

/** The identity dimensions one schema-store session entry carries. */
export interface SessionMetadataIdentity {
  /** Persistent connection config id this session's schema belongs to. */
  connectionId: string;
  /** Session-local config revision; bumped whenever that identity changes. */
  metadataRevision: number;
}

/**
 * Cache key for one relation's columns inside a schema-store session.
 *
 * Identity (connection config + its revision) and runtime session are part of
 * the key, so columns read for one connection config can never be served to
 * another, and the target carries the relation itself.
 */
export function relationColumnsCacheKey(
  session: SessionMetadataIdentity,
  dbSessionId: string,
  ref: RelationRef,
): string {
  return metadataCacheKey(
    identityForSession({
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
    }),
  );
}
