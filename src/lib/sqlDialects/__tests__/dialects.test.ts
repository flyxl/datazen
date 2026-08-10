import { describe, it, expect } from 'vitest';
import { DB_REGISTRY } from '../../databaseTypes';
import { getSqlDialect } from '../index';

describe('getSqlDialect', () => {
  it('maps kiwi to mysql family', () => {
    if (!DB_REGISTRY.kiwi) return; // plugins not injected in this workspace
    expect(getSqlDialect('kiwi')?.family).toBe('mysql');
  });

  it('sqlite DDL query uses sqlite_master', () => {
    const sql = getSqlDialect('sqlite')!.ddl.getTableDdlQuery('users').sql;
    expect(sql).toContain('sqlite_master');
  });

  it('postgresql index supports gin', () => {
    expect(getSqlDialect('postgresql')!.index.supportedIndexMethods).toContain('gin');
  });

  it('mysql index does not support gin', () => {
    expect(getSqlDialect('mysql')!.index.supportedIndexMethods).not.toContain('gin');
  });

  it('mysql drop index includes ON table', () => {
    const sql = getSqlDialect('mysql')!.index.getDropIndexSql('idx_foo', 'users', '`');
    expect(sql).toContain('ON');
  });

  it('postgresql drop index does not include ON table', () => {
    const sql = getSqlDialect('postgresql')!.index.getDropIndexSql('idx_foo', 'users', '"');
    expect(sql).not.toContain('ON');
  });

  it('mysql create index supports hash method and unique', () => {
    const sql = getSqlDialect('mysql')!.index.getCreateIndexSql({
      indexName: 'idx_email',
      tableName: 'users',
      columns: ['email'],
      unique: true,
      method: 'hash',
      quoteChar: '`',
    });
    expect(sql).toContain('UNIQUE INDEX');
    expect(sql).toContain('USING hash');
  });

  it('postgresql create index supports gin method', () => {
    const sql = getSqlDialect('postgresql')!.index.getCreateIndexSql({
      indexName: 'idx_data',
      tableName: 'docs',
      columns: ['data'],
      unique: false,
      method: 'gin',
      quoteChar: '"',
    });
    expect(sql).toContain('USING gin');
  });

  it('sqlite create index omits method clause', () => {
    const sql = getSqlDialect('sqlite')!.index.getCreateIndexSql({
      indexName: 'idx_name',
      tableName: 'items',
      columns: ['name'],
      unique: false,
      method: 'btree',
      quoteChar: '"',
    });
    expect(sql).not.toContain('USING');
    expect(sql).toContain('CREATE INDEX');
  });
});
