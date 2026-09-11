import { beforeEach, describe, expect, it, vi } from 'vitest';
import { MySQL, PostgreSQL, StandardSQL } from '@codemirror/lang-sql';

vi.mock('../../extensions/generated', () => {
  const kiwi = {
    label: 'Kiwi',
    shortLabel: 'Ki',
    iconBg: 'bg-teal-600',
    iconColor: 'text-teal-400',
    defaultPort: 4,
    defaultHost: '',
    defaultUser: '',
    quoteChar: '`',
    connectionMode: 'server',
    supportsSSH: false,
    supportsSSL: false,
    supportsBackup: false,
    supportsTables: true,
    isKeyValue: false,
    supportsSQL: true,
    category: 'sql',
    connectionView: 'sql',
    sqlDialect: 'mysql',
    databaseFieldType: 'name',
    hasMultiDatabase: true,
    connectionForm: 'kiwi',
  };
  const postgresql = {
    label: 'PostgreSQL',
    shortLabel: 'Pg',
    iconBg: 'bg-blue-600',
    iconColor: 'text-blue-400',
    defaultPort: 5432,
    defaultHost: '127.0.0.1',
    defaultUser: 'postgres',
    quoteChar: '"',
    connectionMode: 'server',
    supportsSSH: true,
    supportsSSL: true,
    supportsBackup: true,
    supportsTables: true,
    isKeyValue: false,
    supportsSQL: true,
    category: 'sql',
    connectionView: 'sql',
    sqlDialect: 'postgresql',
    databaseFieldType: 'name',
    connectionForm: 'standard',
    supportsExplain: true,
    hasMultiDatabase: true,
  };
  return {
    DRIVER_DB_ENTRIES: { postgresql, kiwi },
    DRIVER_DB_ENTRIES: { postgresql, kiwi },
    DRIVER_SQL_DIALECTS: {},
  };
});

describe('resolveCmDialect', () => {
  beforeEach(() => {
    vi.resetModules();
  });

  it('maps builtin types directly', async () => {
    const { resolveCmDialect } = await import('../SqlEditor');
    expect(resolveCmDialect('postgresql')).toBe(PostgreSQL);
    expect(resolveCmDialect('mysql')).toBe(MySQL);
    expect(resolveCmDialect(undefined)).toBe(StandardSQL);
  });

  it('maps wapp types via sqlDialect metadata', async () => {
    const { resolveCmDialect } = await import('../SqlEditor');
    expect(resolveCmDialect('kiwi')).toBe(MySQL);
  });

  it('[tester] falls back to StandardSQL for unknown database types', async () => {
    const { resolveCmDialect } = await import('../SqlEditor');
    expect(resolveCmDialect('nonexistent-db')).toBe(StandardSQL);
  });
});
