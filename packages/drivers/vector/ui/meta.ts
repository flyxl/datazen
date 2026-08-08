import type { DatabaseTypeMeta } from '@datazen/plugin-sdk';

export const vectorMeta = {
    label: 'Vector DB',
    shortLabel: 'Vc',
    iconBg: 'bg-emerald-700',
    iconColor: 'text-emerald-400',
    defaultPort: 6333,
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

