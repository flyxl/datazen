import { DB_REGISTRY } from '../databaseTypes';
import type { DatabaseType } from '../../types';
import { postgresqlDialect } from './postgresql';
import { mysqlDialect } from './mysql';
import { sqliteDialect } from './sqlite';
import {
  sqlserverDialect,
  clickhouseDialect,
  duckdbDialect,
  elasticsearchDialect,
  mongodbDialect,
  genericDialect,
  oracleDialect,
} from './extra';
import { DRIVER_SQL_DIALECTS } from '../../extensions/generated';
import type { SqlDialectStrategy } from './types';

const BUILTIN_DIALECTS: Record<string, SqlDialectStrategy> = {
  postgresql: postgresqlDialect,
  mysql: mysqlDialect,
  sqlite: sqliteDialect,
  sqlserver: sqlserverDialect,
  clickhouse: clickhouseDialect,
  duckdb: duckdbDialect,
  elasticsearch: elasticsearchDialect,
  mongodb: mongodbDialect,
  generic: genericDialect,
  oracle: oracleDialect,
};

const DIALECTS: Record<string, SqlDialectStrategy> = {
  ...BUILTIN_DIALECTS,
  ...DRIVER_SQL_DIALECTS,
};

export function getSqlDialect(dbType: DatabaseType): SqlDialectStrategy | null {
  const meta = DB_REGISTRY[dbType];
  if (meta?.sqlDialectStrategy) {
    return meta.sqlDialectStrategy;
  }
  const family = meta?.sqlDialect ?? (dbType as string);
  return family ? (DIALECTS[family] ?? null) : null;
}

export type {
  SqlDialectStrategy,
  SqlDialectFamily,
  TableSqlDialect,
  GeneratedSqlType,
  RowLimitShape,
} from './types';
export { BaseTableSqlGenerator } from './baseTableSql';
export { buildRowLimitedSelect, LIMIT_ROW_LIMIT } from './rowLimit';
export type { RowLimitedSelectParts } from './rowLimit';
