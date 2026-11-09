import { getCachedDDL } from './schemaCache';
import { getSqlDialect } from './sqlDialects';
import {
  exportSingleTableStructureViaRust,
  type SingleTableStructureExportOptions,
} from './batchExportJob';
import type { DatabaseType } from '../types';

export type ExportTableStructureResult = 'saved' | 'cancelled' | 'unsupported';

export interface ExportTableStructureOptions {
  dbSessionId: string;
  tableName: string;
  database: string;
  databaseType: DatabaseType;
  /** Fallback schema for the table (PostgreSQL family). */
  schema?: string | null;
  /** Injectable for tests */
  getDdl?: (
    dbSessionId: string,
    tableName: string,
    sql: string,
    extract: (rows: unknown[][]) => string,
    database: string,
  ) => Promise<string>;
  /** Injectable for tests; defaults to the Rust batch exporter. */
  exportStructure?: (
    options: SingleTableStructureExportOptions,
  ) => Promise<'saved' | 'cancelled'>;
}

/**
 * Thin shell over the Rust batch exporter (item 6): fetch the table DDL
 * with the driver's dialect query, then hand the text to
 * `exportSingleTableStructureViaRust` (`export_tables_stream`,
 * `structure_only`), which opens the native save dialog and writes the
 * file. Returns `unsupported` when the driver has no DDL dialect. The
 * backend never invents DDL: a blank fetch reaches it as a missing `ddl`
 * and comes back as the `-- DDL unavailable for {table}` placeholder,
 * the same content the old direct save wrote.
 */
export async function exportTableStructureToFile(
  options: ExportTableStructureOptions,
): Promise<ExportTableStructureResult> {
  const {
    dbSessionId,
    tableName,
    database,
    databaseType,
    schema,
    getDdl = getCachedDDL,
    exportStructure = exportSingleTableStructureViaRust,
  } = options;

  const dialect = getSqlDialect(databaseType);
  if (!dialect?.ddl?.getTableDdlQuery) {
    return 'unsupported';
  }

  const { sql, extractColumnIndex } = dialect.ddl.getTableDdlQuery(tableName);
  const ddl = await getDdl(
    dbSessionId,
    tableName,
    sql,
    (rows) => {
      const row = rows[0] as unknown[] | undefined;
      const val = row?.[extractColumnIndex];
      return typeof val === 'string' ? val : val != null ? String(val) : '';
    },
    database,
  );

  return exportStructure({
    dbSessionId,
    databaseType,
    database,
    schema: schema ?? undefined,
    tableName,
    ddl,
  });
}
