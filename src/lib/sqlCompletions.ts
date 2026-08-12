import type { Completion } from '@codemirror/autocomplete';

const COMMON: Completion[] = [
  { label: 'COUNT', type: 'function', apply: 'COUNT(' },
  { label: 'SUM', type: 'function', apply: 'SUM(' },
  { label: 'AVG', type: 'function', apply: 'AVG(' },
  { label: 'MIN', type: 'function', apply: 'MIN(' },
  { label: 'MAX', type: 'function', apply: 'MAX(' },
  { label: 'COALESCE', type: 'function', apply: 'COALESCE(' },
  { label: 'NULLIF', type: 'function', apply: 'NULLIF(' },
  { label: 'CAST', type: 'function', apply: 'CAST(' },
  { label: 'CASE', type: 'keyword', apply: 'CASE ' },
  { label: 'EXISTS', type: 'keyword', apply: 'EXISTS (' },
];

const POSTGRES: Completion[] = [
  { label: 'DATE_TRUNC', type: 'function', apply: 'DATE_TRUNC(' },
  { label: 'TO_CHAR', type: 'function', apply: 'TO_CHAR(' },
  { label: 'TO_TIMESTAMP', type: 'function', apply: 'TO_TIMESTAMP(' },
  { label: 'JSONB_BUILD_OBJECT', type: 'function', apply: 'JSONB_BUILD_OBJECT(' },
  { label: 'ARRAY_AGG', type: 'function', apply: 'ARRAY_AGG(' },
  { label: 'STRING_AGG', type: 'function', apply: 'STRING_AGG(' },
  { label: 'GENERATE_SERIES', type: 'function', apply: 'GENERATE_SERIES(' },
  { label: 'NOW', type: 'function', apply: 'NOW()' },
];

const MYSQL: Completion[] = [
  { label: 'IF', type: 'function', apply: 'IF(' },
  { label: 'IFNULL', type: 'function', apply: 'IFNULL(' },
  { label: 'DATE_FORMAT', type: 'function', apply: 'DATE_FORMAT(' },
  { label: 'GROUP_CONCAT', type: 'function', apply: 'GROUP_CONCAT(' },
  { label: 'JSON_EXTRACT', type: 'function', apply: 'JSON_EXTRACT(' },
  { label: 'NOW', type: 'function', apply: 'NOW()' },
];

const SQLITE: Completion[] = [
  { label: 'IFNULL', type: 'function', apply: 'IFNULL(' },
  { label: 'DATETIME', type: 'function', apply: 'DATETIME(' },
  { label: 'PRINTF', type: 'function', apply: 'PRINTF(' },
  { label: 'JSON_EXTRACT', type: 'function', apply: 'JSON_EXTRACT(' },
];

export function sqlFunctionCompletions(databaseType?: string): Completion[] {
  const family = (databaseType ?? '').toLowerCase();
  if (family === 'postgresql' || family === 'postgres' || family === 'cockroach') {
    return [...COMMON, ...POSTGRES];
  }
  if (family === 'mysql' || family === 'mariadb' || family === 'tidb') {
    return [...COMMON, ...MYSQL];
  }
  if (family === 'sqlite') {
    return [...COMMON, ...SQLITE];
  }
  return COMMON;
}
