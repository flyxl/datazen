import type { DatabaseTypeMeta } from '@datazen/plugin-sdk';

export const rqliteMeta = {
    label: 'RQLite',
    shortLabel: 'Rq',
    iconBg: 'bg-accent',
    iconColor: 'text-accent',
    defaultPort: 4001,
    defaultHost: '127.0.0.1',
    defaultUser: '',
    quoteChar: '"',
    connectionMode: 'server',
    supportsSSH: true,
    supportsSSL: true,
    supportsBackup: false,
    supportsTables: true,
    isKeyValue: false,
    supportsSQL: true,
    category: 'sql',
    connectionView: 'sql',
    sqlDialect: 'sqlite',
    databaseFieldType: 'name',
    connectionForm: 'standard',
    supportsExplain: true,
  } satisfies DatabaseTypeMeta;

