import type { DatabaseType, TableSchema } from '../types';
import { DB_REGISTRY, escapeIdent } from './databaseTypes';
import { getSqlDialect, BaseTableSqlGenerator } from './sqlDialects';
import type { GeneratedSqlType } from './sqlDialects/types';

export type { GeneratedSqlType };

export interface SqlGeneratorOptions {
  schemaPrefix?: string;
}

export function formatTableIdentifier(
  tableName: string,
  databaseType: string,
  schemaPrefix?: string,
): string {
  const dbType = databaseType as DatabaseType;
  const dialect = getSqlDialect(dbType);
  if (dialect?.tableSql) {
    return dialect.tableSql.formatTableRef(tableName, schemaPrefix);
  }
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
  const dialect = getSqlDialect(dbType);
  const generator =
    dialect?.tableSql ??
    new BaseTableSqlGenerator(DB_REGISTRY[dbType]?.quoteChar ?? '"');
  const tableRef = generator.formatTableRef(schema.tableName, options?.schemaPrefix);
  return generator.generateSql(type, tableRef, schema);
}
