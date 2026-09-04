import type { DatabaseTypeMeta } from '@datazen/plugin-sdk';

export const influxdbMeta = {
    label: 'InfluxDB',
    shortLabel: 'If',
    iconBg: 'bg-warning',
    iconColor: 'text-warning',
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
    popularityOrder: 17,
    supportsSQL: true,
    category: 'sql',
    connectionView: 'sql',
    sqlDialect: 'generic',
    databaseFieldType: 'name',
    connectionForm: 'standard',
    clipboardSchemes: ['influxdb', 'influx'],
    supportsExplain: false,
  } satisfies DatabaseTypeMeta;

