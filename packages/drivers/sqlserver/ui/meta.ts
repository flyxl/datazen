import type { DatabaseTypeMeta } from '@datazen/plugin-sdk';

export const sqlserverMeta = {
    label: 'SQL Server',
    shortLabel: 'Ss',
    iconBg: 'bg-danger',
    iconColor: 'text-danger',
    defaultPort: 1433,
    defaultHost: '127.0.0.1',
    defaultUser: 'sa',
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
    sqlDialect: 'sqlserver',
    databaseFieldType: 'name',
    connectionForm: 'standard',
    clipboardSchemes: ['sqlserver', 'mssql'],
    supportsExplain: true,
    hasMultiDatabase: true,
  } satisfies DatabaseTypeMeta;

