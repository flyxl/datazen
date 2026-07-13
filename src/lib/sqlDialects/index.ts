import { DB_REGISTRY } from '../databaseTypes';
import type { DatabaseType } from '../../types';
import { postgresqlDialect } from './postgresql';
import { mysqlDialect } from './mysql';
import { sqliteDialect } from './sqlite';
import { trinoDialect } from './trino';
import type { SqlDialectStrategy } from './types';

const DIALECTS = {
  postgresql: postgresqlDialect,
  mysql: mysqlDialect,
  sqlite: sqliteDialect,
  trino: trinoDialect,
};

export function getSqlDialect(dbType: DatabaseType): SqlDialectStrategy | null {
  const family = DB_REGISTRY[dbType]?.sqlDialect;
  return family ? DIALECTS[family] ?? null : null;
}

export type { SqlDialectStrategy, SqlDialectFamily } from './types';
