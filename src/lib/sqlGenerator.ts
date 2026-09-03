import type { DatabaseType, TableSchema } from '../types';
import { escapeIdent } from './databaseTypes';

export type GeneratedSqlType = 'select' | 'insert' | 'update' | 'delete';

export interface SqlGeneratorOptions {
  schemaPrefix?: string;
}

export function formatTableIdentifier(
  tableName: string,
  databaseType: string,
  schemaPrefix?: string,
): string {
  const dbType = databaseType as DatabaseType;
  if (schemaPrefix && schemaPrefix.trim()) {
    return `${escapeIdent(schemaPrefix.trim(), dbType)}.${escapeIdent(tableName, dbType)}`;
  }
  return escapeIdent(tableName, dbType);
}

export function generateTableSql(
  schema: TableSchema,
  type: GeneratedSqlType,
  databaseType: string,
  options?: SqlGeneratorOptions,
): string {
  const dbType = databaseType as DatabaseType;
  const tableRef = formatTableIdentifier(schema.tableName, databaseType, options?.schemaPrefix);

  switch (type) {
    case 'select': {
      const cols = schema.columns.map((c) => escapeIdent(c.name, dbType)).join(', ');
      return `SELECT ${cols}\nFROM ${tableRef};`;
    }
    case 'insert': {
      const insertableCols = schema.columns.filter((c) => !c.isAutoIncrement);
      const colList = insertableCols.map((c) => `  ${escapeIdent(c.name, dbType)}`).join(',\n');
      const valList = insertableCols
        .map((c) => {
          if (c.defaultValue) return `  ${c.defaultValue}`;
          if (c.dataType.toLowerCase().includes('int') || c.dataType.toLowerCase().includes('numeric')) {
            return '  0';
          }
          return "  ''";
        })
        .join(',\n');
      return `INSERT INTO ${tableRef} (\n${colList}\n) VALUES (\n${valList}\n);`;
    }
    case 'update': {
      const nonPkCols = schema.columns.filter(
        (c) => !schema.primaryKeys.includes(c.name) && !c.isPrimaryKey,
      );
      const targetCols = nonPkCols.length > 0 ? nonPkCols : schema.columns;
      const setClauses = targetCols.map((c) => `  ${escapeIdent(c.name, dbType)} = ''`).join(',\n');

      const pks =
        schema.primaryKeys.length > 0
          ? schema.primaryKeys
          : schema.columns.filter((c) => c.isPrimaryKey).map((c) => c.name);

      let whereClause: string;
      if (pks.length > 0) {
        whereClause = pks.map((pk) => `${escapeIdent(pk, dbType)} = `).join(' AND ');
      } else {
        whereClause = '/* WARNING: Primary Key not found. Specify condition */';
      }
      return `UPDATE ${tableRef}\nSET\n${setClauses}\nWHERE ${whereClause};`;
    }
    case 'delete': {
      const pks =
        schema.primaryKeys.length > 0
          ? schema.primaryKeys
          : schema.columns.filter((c) => c.isPrimaryKey).map((c) => c.name);

      let whereClause: string;
      if (pks.length > 0) {
        whereClause = pks.map((pk) => `${escapeIdent(pk, dbType)} = `).join(' AND ');
      } else {
        whereClause = '/* WARNING: Primary Key not found. Specify condition */';
      }
      return `DELETE FROM ${tableRef}\nWHERE ${whereClause};`;
    }
  }
}
