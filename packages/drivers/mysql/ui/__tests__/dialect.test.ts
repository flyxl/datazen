import { describe, expect, it } from 'vitest';
import { mysqlDialect, mysqlDialectProfile } from '../dialect';

describe('MySQL Driver Dialect', () => {
  it('provides correct DDL queries and table sql generator', () => {
    expect(mysqlDialect.family).toBe('mysql');
    const tableDdl = mysqlDialect.ddl.getTableDdlQuery('users');
    expect(tableDdl.sql).toBe('SHOW CREATE TABLE `users`');
    expect(tableDdl.extractColumnIndex).toBe(1);

    const viewDdl = mysqlDialect.ddl.getViewDdlQuery!('user_view');
    expect(viewDdl.sql).toBe('SHOW CREATE VIEW `user_view`');

    expect(mysqlDialect.index.supportedIndexMethods).toEqual(['btree', 'hash']);

    const dropSql = mysqlDialect.index.getDropIndexSql('idx_name', 'users', '`');
    expect(dropSql).toBe('DROP INDEX `idx_name` ON `users`');

    const createSql = mysqlDialect.index.getCreateIndexSql({
      indexName: 'idx_name',
      tableName: 'users',
      columns: ['name'],
      method: 'hash',
      quoteChar: '`',
    });
    expect(createSql).toBe('CREATE INDEX `idx_name` ON `users` USING hash (`name`)');
  });

  it('provides correct semantic profile for mysql editor', () => {
    expect(mysqlDialectProfile.quoteStyle).toBe('backtick');
    expect(mysqlDialectProfile.foldCase).toBe('lower');
    expect(mysqlDialectProfile.projectionAliasVisibility).toBe('order-group');
    expect(mysqlDialectProfile.parameterPolicy.question).toBe(true);
    expect(mysqlDialectProfile.parameterPolicy.dollarPositional).toBe(false);
  });
});
