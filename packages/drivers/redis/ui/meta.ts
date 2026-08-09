import type { DatabaseTypeMeta } from '@datazen/plugin-sdk';

export const redisMeta = {
    label: 'Redis',
    shortLabel: 'Rd',
    iconBg: 'bg-red-600',
    iconColor: 'text-red-400',
    defaultPort: 6379,
    defaultHost: '127.0.0.1',
    defaultUser: '',
    quoteChar: '',
    connectionMode: 'server',
    supportsSSH: true,
    supportsSSL: false,
    supportsBackup: false,
    supportsTables: false,
    isKeyValue: true,
    supportsSQL: false,
    category: 'kv',
    connectionView: 'keyvalue',
    databaseFieldType: 'index',
    connectionForm: 'redis',
  } satisfies DatabaseTypeMeta;

