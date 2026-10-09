/**
 * Dependency-inversion seam between the connection store and the schema store.
 *
 * `schemaStore` transitively depends on `activeConnectionStore` (column
 * loading reads driver capabilities through that store), so a static import in
 * the opposite direction would close a module cycle — and under Vitest the
 * cycle deadlocks collection for every suite that mocks `schemaStore` while
 * the real `activeConnectionStore` is still in the graph. The schema store
 * registers its binder when it initialises; callers only need to announce that
 * a session's metadata identity was observed, which is inert until the store
 * exists.
 */

export type MetadataIdentityBinder = (dbSessionId: string, connectionId: string) => void;

let binder: MetadataIdentityBinder | null = null;

/** Install the schema-store binder; returns a disposer restoring the void. */
export function registerMetadataIdentityBinder(next: MetadataIdentityBinder): () => void {
  binder = next;
  return () => {
    if (binder === next) binder = null;
  };
}

/** Forward a newly observed (session, config) pair to the schema store. */
export function bindSessionMetadataIdentity(dbSessionId: string, connectionId: string): void {
  binder?.(dbSessionId, connectionId);
}
