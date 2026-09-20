import { describe, expect, it } from 'vitest';
import type { DatabaseTypeMeta } from '../databaseMeta';
import { resolveCreateTableSchemaFromMeta } from './resolveCreateTableSchema';
import type { TypeCategory } from '../../components/query-builder/typeCategory';

const PostgreSQLTypeCategories: Record<string, TypeCategory> = {
  int4: 'numeric',
  int8: 'numeric',
  int2: 'numeric',
  numeric: 'numeric',
  decimal: 'numeric',
  real: 'numeric',
  double: 'numeric',
  boolean: 'boolean',
  bool: 'boolean',
  date: 'temporal',
  timestamp: 'temporal',
  timestamptz: 'temporal',
  time: 'temporal',
  text: 'text',
  varchar: 'text',
  char: 'text',
  json: 'jsonb',
  jsonb: 'jsonb',
  bytea: 'binary',
};

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
  qbTypeCategories: PostgreSQLTypeCategories,
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

  it('defaults to public for PostgreSQL without context schema', () => {
    const meta: DatabaseTypeMeta = {
      ...baseMeta,
      databaseFieldType: 'name',
      namespaceEnsure: 'postgresql',
    };
    expect(
      resolveCreateTableSchemaFromMeta(meta, 'postgresql', {
        currentDatabase: 'mydb',
      }),
    ).toBe('public');
  });

  it('never uses database name as schema for PostgreSQL', () => {
    const meta: DatabaseTypeMeta = {
      ...baseMeta,
      databaseFieldType: 'name',
      namespaceEnsure: 'postgresql',
    };
    const result = resolveCreateTableSchemaFromMeta(meta, 'postgresql', {
      currentDatabase: 'mydb',
    });
    expect(result).not.toBe('mydb');
    expect(result).toBe('public');
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
