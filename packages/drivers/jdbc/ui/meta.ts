import type { DatabaseTypeMeta } from '@datazen/driver-sdk';

/** Generic JDBC via external Agent — advanced capabilities are limited vs native drivers. */
export const jdbcMeta = {
  label: 'JDBC',
  shortLabel: 'JDBC',
  iconBg: 'bg-muted',
  iconColor: 'text-muted-foreground',
  defaultPort: 0,
  defaultHost: '',
  defaultUser: '',
  quoteChar: '"',
  connectionMode: 'server',
  supportsSSH: true,
  supportsSSL: false,
  supportsBackup: false,
  supportsTables: true,
  isKeyValue: false,
  popularityOrder: 99,
  supportsSQL: true,
  category: 'sql',
  connectionView: 'sql',
  sqlDialect: 'sql',
  databaseFieldType: 'name',
  connectionForm: 'standard',
  clipboardSchemes: ['jdbc'],
  supportsExplain: false,
} satisfies DatabaseTypeMeta;
