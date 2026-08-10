import { describe, expect, it } from 'vitest';
import type { DatabaseTypeMeta } from '../databaseMeta';
import { resolveCreateTableSchemaFromMeta } from './resolveCreateTableSchema';

const baseMeta = {
  label: 'Test',
  shortLabel: 'T',
  iconBg: '',
  iconColor: '',
  defaultPort: 0,
  defaultHost: '',
  defaultUser: '',
  quoteChar: '"',
  connectionMode: 'server',
  supportsSSH: false,
  supportsSSL: false,
  supportsBackup: false,
  supportsTables: true,
  isKeyValue: false,
  supportsSQL: true,
  category: 'sql',
  connectionView: 'sql',
  connectionForm: 'standard',
} satisfies Omit<DatabaseTypeMeta, 'databaseFieldType' | 'namespaceEnsure'>;

describe('resolveCreateTableSchemaFromMeta', () => {
  it('uses context schema for PostgreSQL, not database name', () => {
    const meta: DatabaseTypeMeta = {
      ...baseMeta,
      databaseFieldType: 'name',
      namespaceEnsure: 'postgresql',
    };
    expect(
      resolveCreateTableSchemaFromMeta(meta, 'postgresql', {
        currentDatabase: 'mydb',
        contextSchema: 'public',
      }),
    ).toBe('public');
  });

  it('returns null for PostgreSQL without context schema (search_path)', () => {
    const meta: DatabaseTypeMeta = {
      ...baseMeta,
      databaseFieldType: 'name',
      namespaceEnsure: 'postgresql',
    };
    expect(
      resolveCreateTableSchemaFromMeta(meta, 'postgresql', {
        currentDatabase: 'mydb',
      }),
    ).toBeNull();
  });

  it('uses database name for MySQL', () => {
    const meta: DatabaseTypeMeta = {
      ...baseMeta,
      databaseFieldType: 'name',
      namespaceEnsure: 'default-sql',
    };
    expect(
      resolveCreateTableSchemaFromMeta(meta, 'mysql', {
        currentDatabase: 'app',
        contextSchema: 'ignored',
      }),
    ).toBe('app');
  });

  it('returns null for SQLite', () => {
    const meta: DatabaseTypeMeta = {
      ...baseMeta,
      databaseFieldType: 'path',
      connectionMode: 'file',
    };
    expect(
      resolveCreateTableSchemaFromMeta(meta, 'sqlite', {
        currentDatabase: '/tmp/test.db',
      }),
    ).toBeNull();
  });
});
