/**
 * Endpoint identity shared by the migration wizards (Schema Diff and Data
 * Sync).
 *
 * Both wizards answer the same question — "is the user pointing the migration
 * at the same place on both ends?" — and the answer decides whether the
 * apply step is allowed to run. The rule lives here once so the two windows
 * cannot drift apart: the connection must be the same, both databases must be
 * named and equal, and the schemas compare after trimming.
 */

export interface EndpointIdentity {
  /** Persisted connection config id (see docs/architecture/naming.md §3). */
  connectionId: string;
  database: string;
  schema: string;
}

/**
 * True when both sides point at the same connection, database and schema.
 *
 * An empty database on either side never matches: "the same place" has to be
 * a named place.
 */
export function isSameEndpointIdentity(
  source: EndpointIdentity,
  target: EndpointIdentity,
): boolean {
  const norm = (value: string) => value.trim();
  return (
    source.connectionId === target.connectionId &&
    source.database !== '' &&
    target.database !== '' &&
    source.database === target.database &&
    norm(source.schema) === norm(target.schema)
  );
}
