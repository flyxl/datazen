import { databaseCommands } from '../commands/database';
import { getCachedTableSchema } from './schemaCache';
import { DB_REGISTRY } from './databaseTypes';
import { formatTableIdentifier, generateTableSql, type GeneratedSqlType } from './sqlGenerator';
import type { DatabaseType, TableSchema } from '../types';

/** Dialects whose drivers parse `schema.table` in get_table_schema / get_columns. */
const SCHEMA_QUALIFIED_DIALECTS = new Set(['postgresql', 'sqlserver']);

/**
 * Build ordered driver table refs to try when loading column metadata.
 * MySQL/SQLite bind bare TABLE_NAME — qualified refs return zero rows.
 */
export function driverTableRefsToTry(
  tableName: string,
  schema: string | undefined,
  databaseType: string,
): string[] {
  const dialect = DB_REGISTRY[databaseType as DatabaseType]?.sqlDialect ?? databaseType;
  const schemaTrimmed = schema?.trim();
  const qualified = schemaTrimmed ? `${schemaTrimmed}.${tableName}` : null;

  if (SCHEMA_QUALIFIED_DIALECTS.has(dialect) && qualified) {
    return qualified === tableName ? [tableName] : [qualified, tableName];
  }
  return [tableName];
}

export function buildPseudoTableSchema(tableName: string, colNames: string[]): TableSchema {
  return {
    tableName,
    columns: colNames.map((c) => ({ name: c, dataType: '', nullable: true })),
    primaryKeys: [],
    indexes: [],
    foreignKeys: [],
  };
}

export async function fetchTableSchemaForSqlGeneration(args: {
  dbSessionId: string;
  tableName: string;
  schema?: string;
  database?: string;
  databaseType: string;
  columnMap?: Record<string, string[]>;
}): Promise<TableSchema | null> {
  const { dbSessionId, tableName, schema, database, databaseType, columnMap } = args;
  const refs = driverTableRefsToTry(tableName, schema, databaseType);

  for (const ref of refs) {
    try {
      const tableSchema = await getCachedTableSchema(dbSessionId, ref, database);
      if (tableSchema.columns.length > 0) {
        return { ...tableSchema, tableName };
      }
    } catch {
      // Try the next ref shape.
    }
  }

  for (const ref of refs) {
    try {
      const colNames = await databaseCommands.getColumns(dbSessionId, ref, database);
      if (colNames.length > 0) {
        return buildPseudoTableSchema(tableName, colNames);
      }
    } catch {
      // Try the next ref shape.
    }
  }

  const cached = columnMap?.[tableName];
  if (cached?.length) {
    return buildPseudoTableSchema(tableName, cached);
  }

  return null;
}

export function generateTableSqlWithFallbacks(
  tableSchema: TableSchema | null,
  type: GeneratedSqlType,
  databaseType: string,
  opts: { schemaPrefix?: string; tableName: string; tableRefLabel?: string },
): string {
  if (tableSchema && tableSchema.columns.length > 0) {
    return generateTableSql(tableSchema, type, databaseType, { schemaPrefix: opts.schemaPrefix });
  }
  const tableId = formatTableIdentifier(opts.tableName, databaseType, opts.schemaPrefix);
  if (type === 'select') {
    return `SELECT *\nFROM ${tableId};`;
  }
  return `/* No column metadata available for ${opts.tableRefLabel ?? opts.tableName} */`;
}
