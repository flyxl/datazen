import { fileCommands } from '../commands/file';
import { getCachedDDL } from './schemaCache';
import { getSqlDialect } from './sqlDialects';
import type { DatabaseType } from '../types';

export type ExportTableStructureResult = 'saved' | 'cancelled' | 'unsupported';

export interface ExportTableStructureOptions {
  connectionId: string;
  tableName: string;
  databaseType: DatabaseType;
  /** Injectable for tests */
  getDdl?: (
    connectionId: string,
    tableName: string,
    sql: string,
    extract: (rows: unknown[][]) => string,
  ) => Promise<string>;
  saveText?: (
    contents: string,
    defaultFileName: string,
    filterName: string,
    extensions: string[],
  ) => Promise<boolean>;
}

/**
 * Fetch table DDL and save via native dialog as `{table}.sql`.
 * Returns `unsupported` when the driver has no DDL dialect.
 */
export async function exportTableStructureToFile(
  options: ExportTableStructureOptions,
): Promise<ExportTableStructureResult> {
  const {
    connectionId,
    tableName,
    databaseType,
    getDdl = getCachedDDL,
    saveText = fileCommands.saveTextWithDialog,
  } = options;

  const dialect = getSqlDialect(databaseType);
  if (!dialect?.ddl?.getTableDdlQuery) {
    return 'unsupported';
  }

  const { sql, extractColumnIndex } = dialect.ddl.getTableDdlQuery(tableName);
  const ddl = await getDdl(connectionId, tableName, sql, (rows) => {
    const row = rows[0] as unknown[] | undefined;
    const val = row?.[extractColumnIndex];
    return typeof val === 'string' ? val : val != null ? String(val) : '';
  });

  const content = ddl.trim() ? ddl : `-- DDL unavailable for ${tableName}`;
  const safeName = tableName.replace(/[^\w.-]+/g, '_') || 'table';
  const saved = await saveText(content, `${safeName}.sql`, 'SQL', ['sql']);
  return saved ? 'saved' : 'cancelled';
}
