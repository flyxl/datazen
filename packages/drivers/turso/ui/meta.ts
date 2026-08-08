import type { DatabaseTypeMeta } from '@datazen/plugin-sdk';

export const tursoMeta = {
    label: 'Turso / libSQL',
    shortLabel: 'Tu',
    iconBg: 'bg-fuchsia-700',
    iconColor: 'text-fuchsia-400',
    defaultPort: 8080,
    defaultHost: '127.0.0.1',
    defaultUser: '',
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
    sqlDialect: 'sqlite',
    databaseFieldType: 'name',
    connectionForm: 'standard',
    supportsExplain: true,
  } satisfies DatabaseTypeMeta;

