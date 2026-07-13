import { describe, it, expect } from 'vitest';
import { getSqlDialect } from '../index';

describe('getSqlDialect', () => {
  it('maps kiwi to mysql family', () => {
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

  it('trino maps presto and trino db types', () => {
    expect(getSqlDialect('trino')?.family).toBe('trino');
    expect(getSqlDialect('presto')?.family).toBe('trino');
    const ddl = getSqlDialect('trino')!.ddl.getTableDdlQuery('users', 'default').sql;
    expect(ddl).toContain('SHOW CREATE TABLE');
  });
});
