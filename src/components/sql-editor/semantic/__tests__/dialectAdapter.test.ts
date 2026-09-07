import { describe, expect, it } from 'vitest';
import { getDialectAdapter, listSupportedDialectIds } from '../dialectAdapter';

describe('getDialectAdapter', () => {
  it('returns adapters for all supported dialect ids', () => {
    const ids = listSupportedDialectIds();
    expect(ids).toContain('postgresql');
    expect(ids).toContain('mysql');
    expect(ids).toContain('sqlite');
    expect(ids).toContain('mssql');
    expect(ids).toContain('duckdb');
    expect(ids).toContain('clickhouse');
    expect(ids).toContain('oracle');
    for (const id of ids) {
      expect(getDialectAdapter(id).dialectId).toBe(id);
    }
  });

  it('falls back to standard profile for unknown dialect', () => {
    const adapter = getDialectAdapter('unknown-db');
    expect(adapter.quoteStyle).toBe('double');
    expect(adapter.foldUnquotedIdentifier('Foo')).toBe('foo');
  });

  it('quotes identifiers with dialect-specific style', () => {
    expect(getDialectAdapter('postgresql').quoteIdentifier('user')).toBe('"user"');
    expect(getDialectAdapter('mysql').quoteIdentifier('user')).toBe('`user`');
    expect(getDialectAdapter('mssql').quoteIdentifier('user')).toBe('[user]');
  });

  it('unquotes double, backtick, and bracket identifiers', () => {
    const pg = getDialectAdapter('postgresql');
    expect(pg.unquoteIdentifier('"a""b"')).toBe('a"b');
    const mysql = getDialectAdapter('mysql');
    expect(mysql.unquoteIdentifier('`col`')).toBe('col');
    const mssql = getDialectAdapter('mssql');
    expect(mssql.unquoteIdentifier('[a]]b]')).toBe('a]b');
  });

  it('folds identifiers per dialect case rules', () => {
    expect(getDialectAdapter('postgresql').foldUnquotedIdentifier('MyTable')).toBe('mytable');
    expect(getDialectAdapter('oracle').foldUnquotedIdentifier('mytable')).toBe('MYTABLE');
    expect(getDialectAdapter('mssql').foldUnquotedIdentifier('MyTable')).toBe('MyTable');
  });

  it('parses qualified names with dots and quotes', () => {
    const pg = getDialectAdapter('postgresql');
    const q = pg.parseQualifiedName('"Schema"."Table"');
    expect(q?.namespacePath).toHaveLength(1);
    expect(q?.namespacePath[0]?.name).toBe('Schema');
    expect(q?.name.name).toBe('Table');
  });

  it('compareIdentifiers respects fold rules', () => {
    const pg = getDialectAdapter('postgresql');
    expect(pg.compareIdentifiers('Foo', 'foo')).toBe(true);
    const oracle = getDialectAdapter('oracle');
    expect(oracle.compareIdentifiers('foo', 'FOO')).toBe(true);
  });

  it('exposes parameter policy per dialect', () => {
    expect(getDialectAdapter('postgresql').parameterPolicy.dollarPositional).toBe(true);
    expect(getDialectAdapter('mysql').parameterPolicy.question).toBe(true);
    expect(getDialectAdapter('mssql').parameterPolicy.atNamed).toBe(true);
  });
});
