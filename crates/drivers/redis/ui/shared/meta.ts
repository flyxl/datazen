// Side effect: register this driver's locale packs in the shared @datazen/ui
// i18n registry. generated.ts imports this module on every build where the
// redis driver is selected, so the pack is wired without host cooperation.
import '../../locales';
import type { DatabaseTypeMeta } from '@datazen/driver-sdk';

export const redisMeta = {
  label: 'Redis',
  shortLabel: 'Rd',
  iconBg: 'bg-danger',
  iconColor: 'text-danger',
  defaultPort: 6379,
  defaultHost: '127.0.0.1',
  defaultUser: '',
  quoteChar: '',
  connectionMode: 'server',
  supportsSSH: true,
  supportsSSL: true,
  defaultSslMode: 'disable',
  supportsBackup: false,
  supportsTables: false,
  isKeyValue: true,
  dbCountsCommand: 'db_sizes',
  popularityOrder: 4,
  supportsSQL: false,
  category: 'kv',
  connectionView: 'keyvalue',
  syncFamily: 'redis',
  databaseFieldType: 'index',
  // KV workspace surfaces Redis fills (host contract F-1 — capability flags, not
  // slot components). Declared here rather than in the host so the host never
  // branches on a driver id, and declared slot by slot so a surface Redis does
  // not contribute to keeps the host's default rendering.
  //
  // `contextBar` is deliberately absent: the 48px context band (db selector, key
  // count, memory, scan budget, action buttons) duplicated what the key browser
  // and the right panel already own, and the panel's own controls now sit in the
  // right panel's tab bar. The host therefore keeps its plain toolbar for Redis
  // panels instead of reserving a band nothing fills.
  kvWorkspace: {
    statusBar: true,
    keyPropsSidebar: true,
    home: true,
  },
  connectionForm: 'redis',
  clipboardSchemes: ['redis', 'rediss', 'redis+tls', 'redis-sentinel', 'sentinel'],
  defaultDatabase: '0',
  maxDatabaseIndex: 15,
  defaultOptions: { topology: 'standalone' },
  structureEditor: {
    enabled: false,
    columnTypes: [],
    defaultColumnType: '',
    fields: {},
    indexMethods: [],
  },
} satisfies DatabaseTypeMeta;
