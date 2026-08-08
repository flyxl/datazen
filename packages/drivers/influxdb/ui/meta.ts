import type { DatabaseTypeMeta } from '@datazen/plugin-sdk';

export const influxdbMeta = {
    label: 'InfluxDB',
    shortLabel: 'If',
    iconBg: 'bg-sky-700',
    iconColor: 'text-sky-400',
    defaultPort: 8086,
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

