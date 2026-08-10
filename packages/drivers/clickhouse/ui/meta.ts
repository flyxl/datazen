import type { DatabaseTypeMeta } from '@datazen/plugin-sdk';

export const clickhouseMeta = {
    label: 'ClickHouse',
    shortLabel: 'Ch',
    iconBg: 'bg-warning',
    iconColor: 'text-warning',
    defaultPort: 8123,
    defaultHost: '127.0.0.1',
    defaultUser: 'default',
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
    sqlDialect: 'clickhouse',
    databaseFieldType: 'name',
    connectionForm: 'standard',
    supportsExplain: true,
    hasMultiDatabase: true,
  } satisfies DatabaseTypeMeta;

