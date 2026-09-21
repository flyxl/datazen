import { useActiveConnectionStore } from '../stores/activeConnectionStore';
import type { DriverCapabilities } from '../types';

/**
 * Runtime driver capabilities for a live db session.
 *
 * The host reports these from the concrete driver (`get_connection_info` →
 * `capabilities`), so the UI never has to guess a driver's shape from its
 * dialect or a static metadata table. `null`/`undefined` means the session info
 * has not been loaded yet — treat that as **unknown**, never as supported.
 */

/**
 * Whether the engine has a real second namespace level (`schema`).
 *
 * Only PostgreSQL and SQL Server do. Every other driver carries the whole
 * namespace in `database` and *rejects* a schema argument, so callers must not
 * synthesise one (e.g. by passing the database name).
 */
export function hasSchemaLevel(
  capabilities: DriverCapabilities | null | undefined,
): boolean | 'unknown' {
  if (!capabilities) return 'unknown';
  return capabilities.hasSchemaLevel === true;
}

/**
 * Whether the dialect accepts `OFFSET` in pagination. Unknown when the session
 * info is not loaded; callers should then fall back to the dialect's own
 * spelling rather than silently dropping the row window.
 */
export function supportsOffset(
  capabilities: DriverCapabilities | null | undefined,
): boolean | 'unknown' {
  if (!capabilities) return 'unknown';
  return capabilities.supportsOffset !== false;
}

/**
 * Runtime capabilities for a live db session, from the active-connection store.
 *
 * Kept here (rather than in a component) so store-level code such as
 * `schemaStore.ensureColumns` can gate a schema argument on the driver's real
 * shape without importing React hooks.
 */
export function capabilitiesForDbSession(
  dbSessionId: string | null | undefined,
): DriverCapabilities | undefined {
  if (!dbSessionId) return undefined;
  const connections = useActiveConnectionStore.getState().connections;
  for (const entry of Object.values(connections)) {
    if (entry.dbSessionId === dbSessionId) return entry.capabilities;
  }
  return undefined;
}

/**
 * The schema argument to send for a relation, or `null`.
 *
 * Drops the schema only when the driver is *known* to have no schema level: a
 * namespace label that is not a schema (path-hierarchy trees put catalog names
 * in `TableInfo.schema`) would be rejected outright by such a driver.
 *
 * When the capability is still unknown the schema is kept. Dropping it would be
 * the worse failure — a schema-aware engine would then resolve the relation in
 * its default namespace and return *wrong* structure silently, whereas an
 * unwanted schema on a schema-less engine fails loudly.
 */
export function relationSchemaFor(
  capabilities: DriverCapabilities | null | undefined,
  schema: string | null | undefined,
): string | null {
  if (hasSchemaLevel(capabilities) === false) return null;
  return schema?.trim() ? schema.trim() : null;
}
