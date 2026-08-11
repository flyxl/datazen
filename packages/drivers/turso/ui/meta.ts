import type { DatabaseTypeMeta } from '@datazen/plugin-sdk';

export const tursoMeta = {
    label: 'Turso / libSQL',
    shortLabel: 'Tu',
    iconBg: 'bg-success',
    iconColor: 'text-success',
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
    clipboardSchemes: ['libsql', 'turso'],
    supportsExplain: true,
  } satisfies DatabaseTypeMeta;

