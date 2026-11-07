import { describe, expect, it } from 'vitest';
import { rowToRecord } from '../rowToRecord';

describe('rowToRecord', () => {
  it('converts positional row array to named Record using column defs', () => {
    const columns = [
      { name: 'id', dataType: 'int4' },
      { name: 'name', dataType: 'varchar' },
    ];
    const row = [1, 'Alice'];
    expect(rowToRecord(row, columns)).toEqual({ id: 1, name: 'Alice' });
  });

  it('returns null for null input', () => {
    expect(rowToRecord(null, [])).toBeNull();
  });

  it('handles empty row and columns', () => {
    expect(rowToRecord([], [])).toEqual({});
  });

  it('handles null values in row', () => {
    const columns = [{ name: 'a', dataType: '' }];
    const row = [null];
    expect(rowToRecord(row, columns)).toEqual({ a: null });
  });
});
