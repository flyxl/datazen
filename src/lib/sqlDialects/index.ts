import { DB_REGISTRY } from '../databaseTypes';
import type { DatabaseType } from '../../types';
import { postgresqlDialect } from './postgresql';
import { mysqlDialect } from './mysql';
import { sqliteDialect } from './sqlite';
import { PLUGIN_SQL_DIALECTS } from '../../plugins/generated';
import type { SqlDialectStrategy } from './types';

const BUILTIN_DIALECTS: Record<string, SqlDialectStrategy> = {
  postgresql: postgresqlDialect,
  mysql: mysqlDialect,
  sqlite: sqliteDialect,
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
