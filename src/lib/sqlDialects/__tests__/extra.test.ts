import { describe, expect, it } from 'vitest';
import {
  clickhouseDialect,
  duckdbDialect,
  elasticsearchDialect,
  genericDialect,
  mongodbDialect,
  sqlserverDialect,
} from '../extra';

describe('extra sql dialects', () => {
  it('sqlserver DDL escapes table name quotes', () => {
    const q = sqlserverDialect.ddl.getTableDdlQuery("users'; DROP--");
    expect(q.sql).toContain("users''; DROP--");
    expect(q.extractColumnIndex).toBe(0);
  });

  it('clickhouse DDL uses system.tables', () => {
    const q = clickhouseDialect.ddl.getTableDdlQuery('events');
    expect(q.sql).toContain('system.tables');
    expect(q.sql).toContain('events');
  });

  it('duckdb DDL uses duckdb_tables()', () => {
    const q = duckdbDialect.ddl.getTableDdlQuery('t1');
    expect(q.sql).toContain('duckdb_tables()');
  });

  it('duckdb drop index omits ON table', () => {
    const sql = duckdbDialect.index.getDropIndexSql('idx', 'users', '"');
    expect(sql).toBe('DROP INDEX "idx"');
  });

  it('sqlserver drop index includes ON table', () => {
    const sql = sqlserverDialect.index.getDropIndexSql('idx', 'users', '"');
    expect(sql).toContain('ON "users"');
  });

  it('creates unique and non-unique indexes', () => {
    const sql = clickhouseDialect.index.getCreateIndexSql({
      indexName: 'idx_a',
      tableName: 't',
      columns: ['a', 'b'],
      quoteChar: '`',
      unique: true,
    });
    expect(sql).toContain('UNIQUE INDEX');
    expect(sql).toContain('`a`, `b`');
  });

  it('no-op DDL for elasticsearch and mongodb', () => {
    for (const d of [elasticsearchDialect, mongodbDialect, genericDialect]) {
      const q = d.ddl.getTableDdlQuery('any');
      expect(q.sql).toContain('WHERE 0');
    }
  });
});
