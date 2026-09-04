import { describe, expect, it } from 'vitest';
import { normalizeDriverId } from '../syncTaxonomy';

describe('[tester] syncTaxonomy', () => {
  it('normalizes wire aliases to registry driver ids', () => {
    expect(normalizeDriverId('Postgres')).toBe('postgresql');
    expect(normalizeDriverId('MSSQL')).toBe('sqlserver');
    expect(normalizeDriverId('presto')).toBe('trino');
    expect(normalizeDriverId('TiDB')).toBe('mysql');
    expect(normalizeDriverId('oceanbase')).toBe('mysql');
  });

  it('passes through unknown ids lowercased', () => {
    expect(normalizeDriverId('ClickHouse')).toBe('clickhouse');
    expect(normalizeDriverId('kiwi')).toBe('kiwi');
  });
});
