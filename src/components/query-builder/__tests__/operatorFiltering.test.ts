/**
 * Operator filtering: the condition dialogs should only offer operators
 * valid for the selected column's data type.
 *
 * - LIKE / NOT LIKE → text/binary only (not numeric, temporal, boolean, json)
 * - IN / NOT IN → all types
 * - Comparison (=, !=, >, <, >=, <=) → all types
 * - IS [NOT] NULL → all types
 */
import { describe, expect, it } from 'vitest';
import { getOperatorOptions } from '../operatorFilter';
import type { SelectOption } from '../../ui/Select';

describe('getOperatorOptions', () => {
  const allOps = [
    '=',
    '!=',
    '>',
    '<',
    '>=',
    '<=',
    'LIKE',
    'NOT LIKE',
    'IN',
    'NOT IN',
    'IS NULL',
    'IS NOT NULL',
  ];

  it('returns all operators when column type is unknown', () => {
    const opts = getOperatorOptions(undefined);
    expect(opts.map((o) => o.value)).toEqual(allOps);
  });

  it('returns all operators when column type is empty string', () => {
    const opts = getOperatorOptions('');
    expect(opts.map((o) => o.value)).toEqual(allOps);
  });

  // ── Numeric columns ─────────────────────────────────────────
  it('excludes LIKE/NOT LIKE for integer columns', () => {
    const opts = getOperatorOptions('integer');
    const values = opts.map((o) => o.value);
    expect(values).not.toContain('LIKE');
    expect(values).not.toContain('NOT LIKE');
  });

  it('excludes LIKE/NOT LIKE for decimal columns', () => {
    const opts = getOperatorOptions('numeric');
    const values = opts.map((o) => o.value);
    expect(values).not.toContain('LIKE');
    expect(values).not.toContain('NOT LIKE');
  });

  it('excludes LIKE/NOT LIKE for float columns', () => {
    const opts = getOperatorOptions('double precision');
    const values = opts.map((o) => o.value);
    expect(values).not.toContain('LIKE');
    expect(values).not.toContain('NOT LIKE');
  });

  // ── Temporal columns ────────────────────────────────────────
  it('excludes LIKE/NOT LIKE for date columns', () => {
    const opts = getOperatorOptions('date');
    const values = opts.map((o) => o.value);
    expect(values).not.toContain('LIKE');
    expect(values).not.toContain('NOT LIKE');
  });

  it('excludes LIKE/NOT LIKE for timestamp columns', () => {
    const opts = getOperatorOptions('timestamp without time zone');
    const values = opts.map((o) => o.value);
    expect(values).not.toContain('LIKE');
    expect(values).not.toContain('NOT LIKE');
  });

  // ── Boolean columns ─────────────────────────────────────────
  it('excludes LIKE/NOT LIKE for boolean columns', () => {
    const opts = getOperatorOptions('boolean');
    const values = opts.map((o) => o.value);
    expect(values).not.toContain('LIKE');
    expect(values).not.toContain('NOT LIKE');
  });

  // ── Text columns ────────────────────────────────────────────
  it('includes LIKE/NOT LIKE for varchar columns', () => {
    const opts = getOperatorOptions('varchar');
    const values = opts.map((o) => o.value);
    expect(values).toContain('LIKE');
    expect(values).toContain('NOT LIKE');
  });

  it('includes LIKE/NOT LIKE for text columns', () => {
    const opts = getOperatorOptions('text');
    const values = opts.map((o) => o.value);
    expect(values).toContain('LIKE');
    expect(values).toContain('NOT LIKE');
  });

  // ── JSON columns ────────────────────────────────────────────
  it('excludes LIKE/NOT LIKE for json/jsonb columns', () => {
    const opts = getOperatorOptions('jsonb');
    const values = opts.map((o) => o.value);
    expect(values).not.toContain('LIKE');
    expect(values).not.toContain('NOT LIKE');
  });

  // ── Binary columns ──────────────────────────────────────────
  it('includes LIKE/NOT LIKE for binary columns', () => {
    const opts = getOperatorOptions('bytea');
    const values = opts.map((o) => o.value);
    expect(values).toContain('LIKE');
    expect(values).toContain('NOT LIKE');
  });

  // ── Common operators always present ─────────────────────────
  it('always includes IS NULL / IS NOT NULL', () => {
    for (const type of ['integer', 'date', 'boolean', 'varchar', 'jsonb', 'bytea']) {
      const opts = getOperatorOptions(type);
      const values = opts.map((o) => o.value);
      expect(values).toContain('IS NULL');
      expect(values).toContain('IS NOT NULL');
    }
  });

  it('always includes IN / NOT IN', () => {
    for (const type of ['integer', 'date', 'boolean', 'varchar', 'jsonb', 'bytea']) {
      const opts = getOperatorOptions(type);
      const values = opts.map((o) => o.value);
      expect(values).toContain('IN');
      expect(values).toContain('NOT IN');
    }
  });

  it('always includes comparison operators', () => {
    for (const type of ['integer', 'date', 'boolean', 'varchar', 'jsonb', 'bytea']) {
      const opts = getOperatorOptions(type);
      const values = opts.map((o) => o.value);
      expect(values).toContain('=');
      expect(values).toContain('!=');
      expect(values).toContain('>');
      expect(values).toContain('<');
      expect(values).toContain('>=');
      expect(values).toContain('<=');
    }
  });
});

describe('getOperatorOptions — auto-reset', () => {
  it('returns a default operator for each category', () => {
    // Default should be '=' for all types
    expect(getOperatorOptions('integer').find((o) => o.value === '=')).toBeTruthy();
    expect(getOperatorOptions('date').find((o) => o.value === '=')).toBeTruthy();
    expect(getOperatorOptions('boolean').find((o) => o.value === '=')).toBeTruthy();
  });
});
