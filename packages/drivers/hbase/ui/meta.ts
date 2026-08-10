import type { DatabaseTypeMeta } from '@datazen/plugin-sdk';

export const hbaseMeta = {
    label: 'HBase',
    shortLabel: 'Hb',
    iconBg: 'bg-warning',
    iconColor: 'text-warning',
    defaultPort: 8080,
    defaultHost: '127.0.0.1',
    defaultUser: '',
    quoteChar: '',
    connectionMode: 'server',
    supportsSSH: true,
    supportsSSL: true,
    supportsBackup: false,
    supportsTables: true,
    isKeyValue: false,
    supportsSQL: true,
    category: 'sql',
    connectionView: 'sql',
    sqlDialect: 'generic',
    databaseFieldType: 'name',
    connectionForm: 'standard',
    supportsExplain: false,
  } satisfies DatabaseTypeMeta;

