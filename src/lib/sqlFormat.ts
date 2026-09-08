import { format } from 'sql-formatter';
import { DB_REGISTRY } from './databaseTypes';
import type { DatabaseType, SqlFormatOptions } from '../types';

const LANGUAGE_MAP: Record<string, string> = {
  postgresql: 'postgresql',
  mysql: 'mysql',
  mariadb: 'mariadb',
  sqlite: 'sqlite',
  sqlserver: 'transactsql',
  tsql: 'transactsql',
};

/** Matches the pre-§4.2 hardcoded behaviour so existing callers are unaffected. */
export const DEFAULT_SQL_FORMAT_OPTIONS: SqlFormatOptions = Object.freeze({
  keywordCase: 'upper',
  indentStyle: '2spaces',
  breakBeforeBooleanOperators: true,
  linesBetweenQueries: 1,
});

export function sqlFormatLanguage(databaseType?: string): string {
  if (!databaseType) return 'sql';
  if (LANGUAGE_MAP[databaseType]) return LANGUAGE_MAP[databaseType];
  const dialect = DB_REGISTRY[databaseType as DatabaseType]?.sqlDialect;
  if (dialect && LANGUAGE_MAP[dialect]) return LANGUAGE_MAP[dialect];
  return 'sql';
}

function indentation(style: SqlFormatOptions['indentStyle']): {
  tabWidth: number;
  useTabs: boolean;
} {
  switch (style) {
    case 'tab':
      return { tabWidth: 1, useTabs: true };
    case '4spaces':
      return { tabWidth: 4, useTabs: false };
    default:
      return { tabWidth: 2, useTabs: false };
  }
}

export function formatSql(
  sql: string,
  databaseType?: string,
  options?: Partial<SqlFormatOptions>,
): string {
  const trimmed = sql.trim();
  if (!trimmed) return sql;
  const resolved = { ...DEFAULT_SQL_FORMAT_OPTIONS, ...options };
  const { tabWidth, useTabs } = indentation(resolved.indentStyle);
  return format(trimmed, {
    language: sqlFormatLanguage(databaseType) as 'sql',
    keywordCase: resolved.keywordCase,
    indentStyle: 'standard',
    tabWidth,
    useTabs,
    logicalOperatorNewline: resolved.breakBeforeBooleanOperators ? 'before' : 'after',
    linesBetweenQueries: Math.max(0, resolved.linesBetweenQueries),
  });
}
