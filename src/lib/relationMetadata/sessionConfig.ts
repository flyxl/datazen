/**
 * Configuration dimension for editor relation-metadata sessions.
 *
 * P4 requires metadata caches to be isolated by identity *and* configuration,
 * not by the runtime session alone: when the connection configuration behind a
 * session changes, everything read under the old configuration must become
 * unreachable. {@link editorSessionCacheKey} is the address that enforces it, and
 * it is derived from {@link metadataSessionKey} — the same identity/config/session
 * prefix {@link metadataCacheKey} uses — so a session partition and a relation
 * cache entry always agree on which configuration they belong to.
 *
 * `src/lib` sits below `src/stores` and must not import it, so the host
 * publishes the configuration of a session through this seam (dependency
 * inversion, the same shape as `src/stores/metadataIdentityBinding.ts`).
 */

import { metadataSessionKey, type MetadataSessionIdentity } from '../metadata';
import type { EditorSessionConfig } from './types';

export type { EditorSessionConfig } from './types';

/** Publishes the configuration a `dbSessionId` currently reads metadata through. */
export type EditorSessionConfigResolver = (
  dbSessionId: string,
) => EditorSessionConfig | null | undefined;

let resolver: EditorSessionConfigResolver | null = null;

/**
 * Install the host's configuration source; the returned function restores the
 * previous one. Suites that never register a resolver get
 * {@link UNBOUND_SESSION_CONFIG} for every session.
 */
export function registerEditorSessionConfigResolver(
  next: EditorSessionConfigResolver | null,
): () => void {
  const previous = resolver;
  resolver = next;
  return () => {
    if (resolver === next) resolver = previous;
  };
}

/**
 * The partition of sessions whose configuration has not been published.
 *
 * This is an isolated address in its own right, not a degraded mode of the
 * configuration dimension: the empty `connectionId` and zero `configRevision`
 * are part of the key, so a bound session can never read what was cached here,
 * and vice versa. It exists because a caller may legitimately ask about a
 * session before the host has bound it to a configuration yet.
 */
export const UNBOUND_SESSION_CONFIG: EditorSessionConfig = Object.freeze({
  connectionId: '',
  configRevision: 0,
});

/** The configuration a session currently reads metadata through. */
export function resolveEditorSessionConfig(dbSessionId: string): EditorSessionConfig {
  if (resolver == null) return UNBOUND_SESSION_CONFIG;
  return resolver(dbSessionId) ?? UNBOUND_SESSION_CONFIG;
}

/**
 * Address of one (identity, configuration, session) partition.
 *
 * Two sessions share a partition only when their `dbSessionId` *and* their whole
 * configuration match; a rebind moves the session to a different address, so no
 * entry, snapshot, debounce timer or in-flight load survives the change.
 */
export function editorSessionCacheKey(
  dbSessionId: string,
  config: EditorSessionConfig = resolveEditorSessionConfig(dbSessionId),
): string {
  return metadataSessionKey({
    connectionId: config.connectionId as MetadataSessionIdentity['connectionId'],
    configRevision: config.configRevision as MetadataSessionIdentity['configRevision'],
    dbSessionId: dbSessionId as MetadataSessionIdentity['dbSessionId'],
  });
}