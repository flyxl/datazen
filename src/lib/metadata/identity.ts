/**
 * Metadata cache identity and isolation keys.
 *
 * P4 contract: metadata batches and cache entries are isolated by
 * *identity* (who requested it), *configuration* (which connection config,
 * at which revision), and *target* (which namespace/object the metadata is
 * about). Two requests that differ in any of these dimensions must never
 * share an entry, a batch, or an in-flight promise.
 */

import type { Counter, ExecutionTarget, Id, NamespaceTarget } from '@datazen/backend-client';

export interface MetadataIdentityInput {
  connectionId: Id;
  configRevision: Counter;
  /** Runtime session the request runs against (never persisted). */
  dbSessionId: Id;
  /** Target namespace/object the metadata describes. */
  target: ExecutionTarget | NamespaceTarget;
}

/** Canonical, stable cache key isolating identity + config + target. */
export function metadataCacheKey(input: MetadataIdentityInput): string {
  const ns = 'namespace' in input.target ? input.target.namespace : input.target;
  const obj = 'object' in input.target ? input.target.object : null;
  return JSON.stringify([
    input.connectionId,
    input.configRevision,
    input.dbSessionId,
    ns.database ?? null,
    ns.catalog ?? null,
    ns.schema ?? null,
    [...ns.path],
    obj ? [obj.kind, obj.name, obj.signature ?? null] : null,
  ]);
}

/** Config-level bucket key: identity + config (no target). */
export function metadataConfigBucket(input: MetadataIdentityInput): string {
  return JSON.stringify([input.connectionId, input.configRevision, input.dbSessionId]);
}

/** Batch bucket key: identity + config + target namespace (object excluded). */
export function metadataBatchBucket(input: MetadataIdentityInput): string {
  const ns = 'namespace' in input.target ? input.target.namespace : input.target;
  return JSON.stringify([
    input.connectionId,
    input.configRevision,
    input.dbSessionId,
    ns.database ?? null,
    ns.catalog ?? null,
    ns.schema ?? null,
    [...ns.path],
  ]);
}
