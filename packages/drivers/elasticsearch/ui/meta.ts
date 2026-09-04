import type { DatabaseTypeMeta } from '@datazen/plugin-sdk';

export const elasticsearchMeta = {
    label: 'Elasticsearch',
    shortLabel: 'Es',
    iconBg: 'bg-warning',
    iconColor: 'text-warning',
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
    popularityOrder: 11,
    supportsSQL: true,
    category: 'sql',
    connectionView: 'sql',
    sqlDialect: 'elasticsearch',
    databaseFieldType: 'name',
    connectionForm: 'standard',
    clipboardSchemes: ['elasticsearch'],
    supportsExplain: false,
  } satisfies DatabaseTypeMeta;

