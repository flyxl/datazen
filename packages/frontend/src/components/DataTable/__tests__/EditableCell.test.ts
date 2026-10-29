import { describe, expect, it } from 'vitest';
import { toEditString } from '../../DataTable/EditableCell';

describe('toEditString', () => {
  it('returns empty string for null', () => {
    expect(toEditString(null)).toBe('');
  });

  it('returns empty string for undefined', () => {
    expect(toEditString(undefined)).toBe('');
  });

  it('stringifies objects as JSON instead of [object Object]', () => {
    const obj = { key: 'value' };
    const result = toEditString(obj);
    expect(result).not.toBe('[object Object]');
    expect(result).toBe('{"key":"value"}');
  });

  it('stringifies arrays as JSON', () => {
    expect(toEditString([1, 2])).toBe('[1,2]');
  });

  it('stringifies nested JSON', () => {
    const nested = { a: { b: 1 }, c: [2, 3] };
    expect(toEditString(nested)).toBe(JSON.stringify(nested));
  });

  it('returns string values as-is', () => {
    expect(toEditString('hello')).toBe('hello');
  });

  it('converts numbers to string', () => {
    expect(toEditString(42)).toBe('42');
  });

  it('converts booleans to string', () => {
    expect(toEditString(true)).toBe('true');
  });
});
