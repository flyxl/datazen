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
    popularityOrder: 19,
    supportsSQL: true,
    category: 'sql',
    connectionView: 'sql',
    sqlDialect: 'generic',
    syncFamily: 'hbase',
    databaseFieldType: 'name',
    connectionForm: 'standard',
    clipboardSchemes: ['hbase'],
    supportsExplain: false,
  } satisfies DatabaseTypeMeta;

