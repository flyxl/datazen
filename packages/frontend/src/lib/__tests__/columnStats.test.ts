import { describe, expect, it } from 'vitest';
import {
  buildColumnStatsSql,
  isAveragableType,
  parseColumnStatsRow,
  type ColumnStatsTable,
} from '../columnStats';

const PG_TABLE: ColumnStatsTable = {
  databaseType: 'postgresql',
  schema: 'public',
  tableName: 'orders',
};

const MYSQL_TABLE: ColumnStatsTable = {
  databaseType: 'mysql',
  schema: null,
  tableName: 'order items',
};

describe('isAveragableType (DB-12)', () => {
  it('accepts the numeric families of the supported dialects', () => {
    for (const type of [
      'integer',
      'int',
      'bigint',
      'smallint',
      'int4',
      'int8',
      'numeric',
      'decimal(10,2)',
      'double precision',
      'float',
      'float8',
      'real',
      'money',
      'serial',
    ]) {
      expect(isAveragableType(type), type).toBe(true);
    }
  });

  it('refuses the types whose AVG is not defined everywhere', () => {
    // AVG(text) does not exist in PostgreSQL: one bad column must not cost
    // the other five values of the statement.
    for (const type of ['varchar', 'text', 'timestamp', 'boolean', 'json', 'bytea', '', null]) {
      expect(isAveragableType(type as string | null), String(type)).toBe(false);
    }
  });
});

describe('buildColumnStatsSql (DB-12)', () => {
  it('projects the six aggregates for a numeric column, schema-qualified', () => {
    expect(buildColumnStatsSql(PG_TABLE, 'price', { includeAvg: true })).toBe(
      'SELECT COUNT(*) AS row_count, COUNT("price") AS non_null_count, ' +
        'COUNT(DISTINCT "price") AS distinct_count, MIN("price") AS min_value, ' +
        'MAX("price") AS max_value, AVG("price") AS avg_value FROM "public"."orders"',
    );
  });

  it('drops the AVG projection for a non-numeric column but keeps the other five', () => {
    expect(buildColumnStatsSql(PG_TABLE, 'note', { includeAvg: false })).toBe(
      'SELECT COUNT(*) AS row_count, COUNT("note") AS non_null_count, ' +
        'COUNT(DISTINCT "note") AS distinct_count, MIN("note") AS min_value, ' +
        'MAX("note") AS max_value FROM "public"."orders"',
    );
  });

  it('quotes per dialect: backticks and embedded quotes survive', () => {
    const sql = buildColumnStatsSql(MYSQL_TABLE, 'qty`x', { includeAvg: true });
    expect(sql).toContain('FROM `order items`');
    expect(sql).toContain('COUNT(DISTINCT `qty``x`)');
  });
});

describe('parseColumnStatsRow (DB-12)', () => {
  it('parses the row in projection order and derives the null count', () => {
    const stats = parseColumnStatsRow([123456, 123450, 42, 'a', 'z', 19.5], {
      includeAvg: true,
    });
    expect(stats).toEqual({
      rowCount: 123456,
      nonNullCount: 123450,
      nullCount: 6,
      distinctCount: 42,
      min: 'a',
      max: 'z',
      avg: 19.5,
      supportsAvg: true,
    });
  });

  it('reads string counts (wide decimal drivers) as numbers', () => {
    const stats = parseColumnStatsRow(['100000000000', '4', '3', null, null], {
      includeAvg: false,
    });
    expect(stats.rowCount).toBe(100000000000);
    expect(stats.nullCount).toBe(99999999996);
    expect(stats.supportsAvg).toBe(false);
    expect(stats.avg).toBeNull();
  });

  it('a missing row or unusable counts read as null, never as zero', () => {
    expect(parseColumnStatsRow(null, { includeAvg: false }).rowCount).toBeNull();
    const stats = parseColumnStatsRow([null, null, null, null, null], { includeAvg: false });
    expect(stats.rowCount).toBeNull();
    expect(stats.nonNullCount).toBeNull();
    // Without both sides the subtraction is not honest either.
    expect(stats.nullCount).toBeNull();
  });
});
