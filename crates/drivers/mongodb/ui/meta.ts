// Side effect: register this driver's locale packs in the shared @datazen/ui
// i18n registry. generated.ts imports this module on every build where the
// mongodb driver is selected, so the pack is wired without host cooperation.
import '../locales';
import type { DatabaseTypeMeta } from '@datazen/driver-sdk';

export const mongodbMeta = {
  label: 'MongoDB',
  shortLabel: 'Mb',
  iconBg: 'bg-success',
  iconColor: 'text-success',
  defaultPort: 27017,
  defaultHost: '127.0.0.1',
  defaultUser: '',
  quoteChar: '',
  connectionMode: 'server',
  supportsSSH: true,
  supportsSSL: true,
  supportsBackup: false,
  supportsTables: true,
  isKeyValue: false,
  popularityOrder: 5,
  supportsSQL: false,
  category: 'document',
  connectionView: 'document',
  sqlDialect: 'mongodb',
  databaseFieldType: 'name',
  connectionForm: 'standard',
  clipboardSchemes: ['mongodb', 'mongodb+srv'],
  supportsExplain: false,
  hasMultiDatabase: true,
  structureEditor: {
    enabled: false,
    columnTypes: [],
    defaultColumnType: '',
    fields: {},
    indexMethods: [],
  },
} satisfies DatabaseTypeMeta;
