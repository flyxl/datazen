import { describe, expect, it } from 'vitest';
import { sqliteDialect, sqliteDialectProfile } from '../dialect';

describe('SQLite Driver Dialect', () => {
  it('provides correct DDL queries and table sql generator', () => {
    expect(sqliteDialect.family).toBe('sqlite');
    const tableDdl = sqliteDialect.ddl.getTableDdlQuery('users');
    expect(tableDdl.sql).toContain("sqlite_master WHERE type='table' AND name='users'");

    const viewDdl = sqliteDialect.ddl.getViewDdlQuery!('user_view');
    expect(viewDdl.sql).toContain("sqlite_master WHERE type='view' AND name='user_view'");

    expect(sqliteDialect.index.supportedIndexMethods).toEqual(['btree']);

    const dropSql = sqliteDialect.index.getDropIndexSql('idx_name', 'users', '"');
    expect(dropSql).toBe('DROP INDEX "idx_name"');

    const createSql = sqliteDialect.index.getCreateIndexSql({
      indexName: 'idx_name',
      tableName: 'users',
      columns: ['name'],
      quoteChar: '"',
    });
    expect(createSql).toBe('CREATE INDEX "idx_name" ON "users" ("name")');
  });

  it('provides correct semantic profile for sqlite editor', () => {
    expect(sqliteDialectProfile.quoteStyle).toBe('double');
    expect(sqliteDialectProfile.foldCase).toBe('lower');
    expect(sqliteDialectProfile.projectionAliasVisibility).toBe('select-only');
    expect(sqliteDialectProfile.parameterPolicy.question).toBe(true);
  });
});
