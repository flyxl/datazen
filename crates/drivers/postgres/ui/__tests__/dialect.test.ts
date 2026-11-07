import { describe, expect, it } from 'vitest';
import { postgresqlDialect, postgresqlDialectProfile } from '../dialect';

describe('PostgreSQL Driver Dialect', () => {
  it('provides correct DDL queries and table sql generator', () => {
    expect(postgresqlDialect.family).toBe('postgresql');
    const tableDdl = postgresqlDialect.ddl.getTableDdlQuery('users');
    expect(tableDdl.sql).toContain('CREATE TABLE');
    expect(tableDdl.sql).toContain("tablename = 'users'");

    const viewDdl = postgresqlDialect.ddl.getViewDdlQuery!('user_view');
    expect(viewDdl.sql).toContain('CREATE OR REPLACE VIEW');

    expect(postgresqlDialect.index.supportedIndexMethods).toContain('gin');
    expect(postgresqlDialect.index.supportedIndexMethods).toContain('gist');

    const dropSql = postgresqlDialect.index.getDropIndexSql('idx_name', 'users', '"');
    expect(dropSql).toBe('DROP INDEX "idx_name"');

    const createSql = postgresqlDialect.index.getCreateIndexSql({
      indexName: 'idx_name',
      tableName: 'users',
      columns: ['name'],
      method: 'gin',
      quoteChar: '"',
    });
    expect(createSql).toBe('CREATE INDEX "idx_name" ON "users" USING gin ("name")');
  });

  it('provides correct semantic profile for postgresql editor', () => {
    expect(postgresqlDialectProfile.quoteStyle).toBe('double');
    expect(postgresqlDialectProfile.foldCase).toBe('lower');
    expect(postgresqlDialectProfile.projectionAliasVisibility).toBe('select-only');
    expect(postgresqlDialectProfile.parameterPolicy.dollarPositional).toBe(true);
    expect(postgresqlDialectProfile.parameterPolicy.atNamed).toBe(false);
  });
});
