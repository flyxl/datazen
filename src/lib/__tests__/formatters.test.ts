import { describe, expect, it } from 'vitest';
import {
  displayValueForTitle,
  formatCell,
  formatLastConnected,
  formatResultCell,
  formatTimestamp,
} from '../formatters';

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

describe('formatResultCell', () => {
  it('returns "NULL" for null', () => {
    expect(formatResultCell(null)).toBe('NULL');
  });

  it('returns "NULL" for undefined', () => {
    expect(formatResultCell(undefined)).toBe('NULL');
  });

  it('stringifies objects as JSON instead of [object Object]', () => {
    const obj = { name: 'test', count: 42 };
    const result = formatResultCell(obj);
    expect(result).not.toBe('[object Object]');
    expect(result).toBe(JSON.stringify(obj));
  });

  it('stringifies arrays as JSON instead of joining', () => {
    const arr = [1, 2, 3];
    const result = formatResultCell(arr);
    expect(result).toBe('[1,2,3]');
  });

  it('returns string values as-is', () => {
    expect(formatResultCell('hello')).toBe('hello');
  });

  it('converts numbers to string', () => {
    expect(formatResultCell(42)).toBe('42');
  });

  it('converts booleans to string', () => {
    expect(formatResultCell(true)).toBe('true');
    expect(formatResultCell(false)).toBe('false');
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

describe('displayValueForTitle', () => {
  it('delegates to formatCell', () => {
    expect(displayValueForTitle({ a: 1 })).toBe('{"a":1}');
  });
});
