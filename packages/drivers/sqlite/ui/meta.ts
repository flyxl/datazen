import type { DatabaseTypeMeta } from '@datazen/plugin-sdk';

export const sqliteMeta = {
    label: 'SQLite',
    shortLabel: 'Lt',
    iconBg: 'bg-emerald-600',
    iconColor: 'text-green-400',
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
    sqlDialect: 'sqlite',
    databaseFieldType: 'path',
    connectionForm: 'file',
    supportsExplain: true,
  } satisfies DatabaseTypeMeta;

