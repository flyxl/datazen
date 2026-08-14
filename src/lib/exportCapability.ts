import type { DatabaseTypeMeta } from './databaseMeta';

/**
 * Data-export capability strength for a connection/driver, weakest to strongest.
 * Named `DataExportCapability` (not `ExportScope`) to avoid colliding with the
 * `ExportScope` type in `exportData.ts` (`current_page` | `selected` | `entire_table`).
 */
export type DataExportCapability = 'none' | 'loaded_only' | 'full_table';

/** Strength order of {@link DataExportCapability}. Lower index = weaker/more restricted. */
const SCOPE_RANK: Record<DataExportCapability, number> = {
  none: 0,
  loaded_only: 1,
  full_table: 2,
};

/** The export capability a driver declares. Absent metadata defaults to full_table. */
export function driverExportScope(dbMeta?: DatabaseTypeMeta): DataExportCapability {
  return dbMeta?.exportScope ?? 'full_table';
}

/**
 * Resolve the effective export capability for a connection.
 *
 * Export is a *read* operation, so a connection's read-only flag does NOT relax
 * or tighten export ability — it only guards mutating SQL / row edits. The
 * effective capability therefore comes solely from the driver metadata.
 */
export function resolveExportScope(dbMeta?: DatabaseTypeMeta): DataExportCapability {
  return driverExportScope(dbMeta);
}

/** Whether this capability can pull/export an entire table (vs already-loaded rows). */
export function supportsFullTableExport(capability: DataExportCapability): boolean {
  return capability === 'full_table';
}

/** Whether this capability permits exporting any data at all. */
export function supportsAnyExport(capability: DataExportCapability): boolean {
  return capability !== 'none';
}

/** Pick the most restricted of two capabilities (driver + instance constraints). */
export function minScope(a: DataExportCapability, b: DataExportCapability): DataExportCapability {
  return SCOPE_RANK[a] <= SCOPE_RANK[b] ? a : b;
}
