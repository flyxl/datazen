import { describe, expect, it } from 'vitest';
import { matchingColumns, tableMatchesObjectSearch } from '../schemaObjectSearch';

describe('schemaObjectSearch', () => {
  it('matches table names', () => {
    expect(tableMatchesObjectSearch('users', 'user')).toBe(true);
    expect(tableMatchesObjectSearch('orders', 'user')).toBe(false);
  });

  it('matches columns when query length >= 2', () => {
    expect(tableMatchesObjectSearch('orders', 'email', ['id', 'email'])).toBe(true);
    expect(tableMatchesObjectSearch('orders', 'em', ['id', 'email'])).toBe(true);
    // Single-char queries only match table/view names, not columns.
    expect(tableMatchesObjectSearch('orders', 'x', ['id', 'email', 'xyz'])).toBe(false);
    expect(tableMatchesObjectSearch('orders', 'phone', ['id', 'email'])).toBe(false);
  });

  it('lists matching columns', () => {
    expect(matchingColumns('id', ['user_id', 'id', 'name'])).toEqual(['user_id', 'id']);
  });
});
