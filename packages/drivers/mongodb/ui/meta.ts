import type { DatabaseTypeMeta } from '@datazen/plugin-sdk';

export const mongodbMeta = {
    label: 'MongoDB',
    shortLabel: 'Mb',
    iconBg: 'bg-green-700',
    iconColor: 'text-green-400',
    defaultPort: 27017,
    defaultHost: '127.0.0.1',
    defaultUser: '',
    quoteChar: '',
    connectionMode: 'server',
    supportsSSH: true,
    supportsSSL: true,
    supportsBackup: false,
    supportsTables: true,
    isKeyValue: false,
    supportsSQL: false,
    category: 'document',
    connectionView: 'document',
    sqlDialect: 'mongodb',
    databaseFieldType: 'name',
    connectionForm: 'standard',
    supportsExplain: false,
    hasMultiDatabase: true,
  } satisfies DatabaseTypeMeta;

