import type { DatabaseTypeMeta } from '@datazen/plugin-sdk';

export const victoriametricsMeta = {
    label: 'VictoriaMetrics',
    shortLabel: 'Vm',
    iconBg: 'bg-accent',
    iconColor: 'text-accent',
    defaultPort: 8428,
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

