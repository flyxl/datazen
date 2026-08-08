import type { DatabaseTypeMeta } from '@datazen/plugin-sdk';

export const duckdbMeta = {
    label: 'DuckDB',
    shortLabel: 'Dk',
    iconBg: 'bg-violet-600',
    iconColor: 'text-violet-400',
    defaultPort: 0,
    defaultHost: '',
    defaultUser: '',
    quoteChar: '"',
    connectionMode: 'file',
    supportsSSH: false,
    supportsSSL: false,
    supportsBackup: false,
    supportsTables: true,
    isKeyValue: false,
    supportsSQL: true,
    category: 'sql',
    connectionView: 'sql',
    sqlDialect: 'duckdb',
    databaseFieldType: 'path',
    connectionForm: 'file',
    supportsExplain: true,
  } satisfies DatabaseTypeMeta;

