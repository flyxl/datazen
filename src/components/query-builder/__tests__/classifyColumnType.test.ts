/**
 * classifyColumnType: map raw database dataType strings to semantic categories
 * so formatValue can decide quoting strategy without driver-specific logic.
 */
import { describe, expect, it } from 'vitest';
import { classifyColumnType, TypeCategory } from '../typeCategory';

describe('classifyColumnType', () => {
  // ── Numeric ─────────────────────────────────────────────────
  it('classifies integer types as numeric', () => {
    expect(classifyColumnType('int')).toBe('numeric');
    expect(classifyColumnType('integer')).toBe('numeric');
    expect(classifyColumnType('bigint')).toBe('numeric');
    expect(classifyColumnType('smallint')).toBe('numeric');
    expect(classifyColumnType('tinyint')).toBe('numeric');
    expect(classifyColumnType('int4')).toBe('numeric');
    expect(classifyColumnType('int8')).toBe('numeric');
    expect(classifyColumnType('serial')).toBe('numeric');
    expect(classifyColumnType('bigserial')).toBe('numeric');
  });

  it('classifies decimal/float types as numeric', () => {
    expect(classifyColumnType('decimal')).toBe('numeric');
    expect(classifyColumnType('numeric')).toBe('numeric');
    expect(classifyColumnType('float')).toBe('numeric');
    expect(classifyColumnType('float8')).toBe('numeric');
    expect(classifyColumnType('double precision')).toBe('numeric');
    expect(classifyColumnType('real')).toBe('numeric');
    expect(classifyColumnType('money')).toBe('numeric');
  });

  // ── Temporal ────────────────────────────────────────────────
  it('classifies date/time types as temporal', () => {
    expect(classifyColumnType('date')).toBe('temporal');
    expect(classifyColumnType('time')).toBe('temporal');
    expect(classifyColumnType('timestamp')).toBe('temporal');
    expect(classifyColumnType('timestamp without time zone')).toBe('temporal');
    expect(classifyColumnType('timestamp with time zone')).toBe('temporal');
    expect(classifyColumnType('timestamptz')).toBe('temporal');
    expect(classifyColumnType('datetime')).toBe('temporal');
    expect(classifyColumnType('datetime2')).toBe('temporal');
    expect(classifyColumnType('smalldatetime')).toBe('temporal');
    expect(classifyColumnType('timetz')).toBe('temporal');
  });

  // ── Boolean ─────────────────────────────────────────────────
  it('classifies boolean types as boolean', () => {
    expect(classifyColumnType('boolean')).toBe('boolean');
    expect(classifyColumnType('bool')).toBe('boolean');
  });

  // ── Text ────────────────────────────────────────────────────
  it('classifies string types as text', () => {
    expect(classifyColumnType('varchar')).toBe('text');
    expect(classifyColumnType('character varying')).toBe('text');
    expect(classifyColumnType('char')).toBe('text');
    expect(classifyColumnType('text')).toBe('text');
    expect(classifyColumnType('nvarchar')).toBe('text');
    expect(classifyColumnType('ntext')).toBe('text');
    expect(classifyColumnType('clob')).toBe('text');
    expect(classifyColumnType('longtext')).toBe('text');
    expect(classifyColumnType('mediumtext')).toBe('text');
  });

  // ── Binary ──────────────────────────────────────────────────
  it('classifies binary types as binary', () => {
    expect(classifyColumnType('blob')).toBe('binary');
    expect(classifyColumnType('bytea')).toBe('binary');
    expect(classifyColumnType('binary')).toBe('binary');
    expect(classifyColumnType('varbinary')).toBe('binary');
    expect(classifyColumnType('image')).toBe('binary');
  });

  // ── JSON ────────────────────────────────────────────────────
  it('classifies json types as json', () => {
    expect(classifyColumnType('json')).toBe('json');
    expect(classifyColumnType('jsonb')).toBe('json');
  });

  // ── UUID ────────────────────────────────────────────────────
  it('classifies uuid as text (string-like)', () => {
    expect(classifyColumnType('uuid')).toBe('text');
  });

  // ── Unknown / empty ────────────────────────────────────────
  it('returns text for empty or unrecognized types', () => {
    expect(classifyColumnType('')).toBe('text');
    expect(classifyColumnType('unknown_type_xyz')).toBe('text');
    expect(classifyColumnType('enum')).toBe('text');
  });

  // ── Case insensitive ───────────────────────────────────────
  it('is case insensitive', () => {
    expect(classifyColumnType('INTEGER')).toBe('numeric');
    expect(classifyColumnType('Date')).toBe('temporal');
    expect(classifyColumnType('BOOLEAN')).toBe('boolean');
    expect(classifyColumnType('VARCHAR')).toBe('text');
  });

  // ── Types with precision / size ─────────────────────────────
  it('handles types with precision specifiers', () => {
    expect(classifyColumnType('varchar(255)')).toBe('text');
    expect(classifyColumnType('decimal(10,2)')).toBe('numeric');
    expect(classifyColumnType('char(36)')).toBe('text');
    expect(classifyColumnType('timestamp(6)')).toBe('temporal');
  });
});

describe('TypeCategory constant', () => {
  it('exports all category values', () => {
    expect(TypeCategory.Numeric).toBe('numeric');
    expect(TypeCategory.Temporal).toBe('temporal');
    expect(TypeCategory.Boolean).toBe('boolean');
    expect(TypeCategory.Text).toBe('text');
    expect(TypeCategory.Binary).toBe('binary');
    expect(TypeCategory.Json).toBe('json');
  });
});
