import { describe, expect, it } from 'vitest';
import { formatCell, formatLastConnected, formatTimestamp } from '../formatters';

describe('formatCell', () => {
  it('returns "NULL" for null', () => {
    expect(formatCell(null)).toBe('NULL');
  });

  it('returns "NULL" for undefined', () => {
    expect(formatCell(undefined)).toBe('NULL');
  });

  it('returns "true"/"false" for booleans', () => {
    expect(formatCell(true)).toBe('true');
    expect(formatCell(false)).toBe('false');
  });

  it('stringifies plain objects as JSON', () => {
    expect(formatCell({ key: 'value' })).toBe('{"key":"value"}');
  });

  it('stringifies arrays as JSON', () => {
    expect(formatCell([1, 2, 3])).toBe('[1,2,3]');
  });

  it('stringifies nested objects as JSON', () => {
    const nested = { a: { b: [1, 2] }, c: 'test' };
    expect(formatCell(nested)).toBe(JSON.stringify(nested));
  });

  it('returns string values as-is', () => {
    expect(formatCell('hello')).toBe('hello');
  });

  it('converts numbers to string', () => {
    expect(formatCell(42)).toBe('42');
    expect(formatCell(3.14)).toBe('3.14');
  });
});

describe('formatTimestamp', () => {
  it('formats valid ISO strings', () => {
    expect(formatTimestamp('2024-01-15T10:00:00Z')).toMatch(/2024-01-15/);
  });

  it('returns NULL for nullish', () => {
    expect(formatTimestamp(null)).toBe('NULL');
  });

  it('returns raw string for invalid dates', () => {
    expect(formatTimestamp('not-a-date')).toBe('not-a-date');
  });
});

describe('formatLastConnected', () => {
  it('returns never-connected label when missing', () => {
    expect(formatLastConnected()).toMatch(/never|Never|未/i);
  });

  it('formats valid ISO timestamp', () => {
    const formatted = formatLastConnected('2024-06-01T12:00:00.000Z');
    expect(formatted.length).toBeGreaterThan(4);
  });
});
