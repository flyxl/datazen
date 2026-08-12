import { format } from 'sql-formatter';
import { DB_REGISTRY } from './databaseTypes';
import type { DatabaseType } from '../types';

const LANGUAGE_MAP: Record<string, string> = {
  postgresql: 'postgresql',
  mysql: 'mysql',
  mariadb: 'mariadb',
  sqlite: 'sqlite',
  sqlserver: 'transactsql',
  tsql: 'transactsql',
};

export function sqlFormatLanguage(databaseType?: string): string {
  if (!databaseType) return 'sql';
  if (LANGUAGE_MAP[databaseType]) return LANGUAGE_MAP[databaseType];
  const dialect = DB_REGISTRY[databaseType as DatabaseType]?.sqlDialect;
  if (dialect && LANGUAGE_MAP[dialect]) return LANGUAGE_MAP[dialect];
  return 'sql';
}

export function formatSql(sql: string, databaseType?: string): string {
  const trimmed = sql.trim();
  if (!trimmed) return sql;
  return format(trimmed, {
    language: sqlFormatLanguage(databaseType) as 'sql',
    keywordCase: 'upper',
    indentStyle: 'standard',
  });
}
