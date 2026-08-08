import type { DatabaseTypeMeta } from '@datazen/plugin-sdk';

export const elasticsearchMeta = {
    label: 'Elasticsearch',
    shortLabel: 'Es',
    iconBg: 'bg-amber-600',
    iconColor: 'text-amber-400',
    defaultPort: 9200,
    defaultHost: '127.0.0.1',
    defaultUser: 'elastic',
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
    sqlDialect: 'elasticsearch',
    databaseFieldType: 'name',
    connectionForm: 'standard',
    supportsExplain: false,
  } satisfies DatabaseTypeMeta;

