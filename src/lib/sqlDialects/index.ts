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
} from './extra';
import { PLUGIN_SQL_DIALECTS } from '../../plugins/generated';
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
};

const DIALECTS: Record<string, SqlDialectStrategy> = {
  ...BUILTIN_DIALECTS,
  ...PLUGIN_SQL_DIALECTS,
};

export function getSqlDialect(dbType: DatabaseType): SqlDialectStrategy | null {
  const family = DB_REGISTRY[dbType]?.sqlDialect;
  return family ? DIALECTS[family] ?? null : null;
}

export type { SqlDialectStrategy, SqlDialectFamily } from './types';
