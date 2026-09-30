import { schemaClient } from '@datazen/driver-sdk';
import { capabilitiesForDbSession, relationSchemaFor } from './driverCapabilities';
import { getCachedTableSchema } from './schemaCache';
import { formatTableIdentifier, generateTableSql, type GeneratedSqlType } from './sqlGenerator';
import type { TableSchema } from '../types';

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
  schema?: string | null;
  database: string;
  databaseType: string;
  columnMap?: Record<string, string[]>;
}): Promise<TableSchema | null> {
  const { dbSessionId, tableName, schema, database, columnMap } = args;
  const relation = {
    database,
    schema: relationSchemaFor(capabilitiesForDbSession(dbSessionId), schema),
    name: tableName,
  };
  try {
    const tableSchema = await getCachedTableSchema(
      dbSessionId,
      tableName,
      database,
      relation.schema,
    );
    if (tableSchema.columns.length > 0) return { ...tableSchema, tableName };
  } catch {
    // Drivers may support columns without full structure metadata.
  }
  try {
    const response = await schemaClient.readColumns(dbSessionId, [relation]);
    const row = response.results.find(
      (row) =>
        row.status === 'ok' &&
        row.value.ref.database === database &&
        row.value.ref.schema === relation.schema &&
        row.value.ref.name === tableName,
    );
    if (row?.status === 'ok' && row.value.columns.length > 0) {
      return {
        ...buildPseudoTableSchema(tableName, []),
        columns: row.value.columns,
        primaryKeys: row.value.primaryKeys,
      };
    }
  } catch {
    // Keep the caller's already loaded columns available on transport failure.
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
  opts: { schemaPrefix?: string | null; tableName: string; tableRefLabel?: string },
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
