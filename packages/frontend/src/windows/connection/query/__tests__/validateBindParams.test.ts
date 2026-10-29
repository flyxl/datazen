import { describe, it, expect } from 'vitest';
import { findMissingParams } from '../validateBindParams';

describe('findMissingParams', () => {
  it('returns empty array when SQL contains no parameters', () => {
    const missing = findMissingParams('SELECT * FROM users WHERE active = true');
    expect(missing).toEqual([]);
  });

  it('detects missing colon named parameters', () => {
    const sql = 'SELECT * FROM users WHERE id = :id AND status = :status';
    const missing = findMissingParams(sql, { 'named:id': '123' });
    expect(missing).toHaveLength(1);
    expect(missing[0].param.name).toBe('status');
    expect(missing[0].label).toBe(':status');
  });

  it('recognizes bare name in rawValues', () => {
    const sql = 'SELECT * FROM users WHERE id = :id';
    const missing = findMissingParams(sql, { id: '456' });
    expect(missing).toEqual([]);
  });

  it('treats empty string or whitespace as missing', () => {
    const sql = 'SELECT * FROM users WHERE id = :id';
    expect(findMissingParams(sql, { 'named:id': '' })).toHaveLength(1);
    expect(findMissingParams(sql, { 'named:id': '   ' })).toHaveLength(1);
  });

  it('treats null or undefined as missing', () => {
    const sql = 'SELECT * FROM users WHERE id = :id';
    expect(findMissingParams(sql, { 'named:id': null })).toHaveLength(1);
    expect(findMissingParams(sql, { 'named:id': undefined })).toHaveLength(1);
    expect(findMissingParams(sql, {})).toHaveLength(1);
    expect(findMissingParams(sql, null)).toHaveLength(1);
  });

  it('accepts 0 and false as valid provided values', () => {
    const sql = 'SELECT * FROM users WHERE count = :cnt AND is_admin = :admin';
    const missing = findMissingParams(sql, {
      'named:cnt': 0,
      'named:admin': false,
    });
    expect(missing).toEqual([]);
  });

  it('accepts explicit "NULL" string as valid provided value', () => {
    const sql = 'SELECT * FROM users WHERE deleted_at = :del';
    const missing = findMissingParams(sql, { 'named:del': 'NULL' });
    expect(missing).toEqual([]);
  });

  it('handles positional parameters ($1, ?)', () => {
    const sql = 'SELECT * FROM users WHERE id = $1 AND name = ?';
    const missing = findMissingParams(sql, { 'dollar:1': '10' });
    expect(missing).toHaveLength(1);
    expect(missing[0].label).toBe('?');
  });
});
