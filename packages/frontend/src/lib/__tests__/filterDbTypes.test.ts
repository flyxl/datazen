import { describe, expect, it } from 'vitest';
import { filterDbTypesByQuery } from '../filterDbTypes';

const items = [
  { value: 'postgresql', label: 'PostgreSQL' },
  { value: 'mysql', label: 'MySQL' },
  { value: 'mongodb', label: 'MongoDB' },
];

describe('filterDbTypesByQuery', () => {
  it('returns all when query empty/whitespace', () => {
    expect(filterDbTypesByQuery(items, '')).toEqual(items);
    expect(filterDbTypesByQuery(items, '  ')).toEqual(items);
  });

  it('matches label case-insensitively', () => {
    expect(filterDbTypesByQuery(items, 'mongo')).toEqual([items[2]]);
  });

  it('matches databaseType id', () => {
    expect(filterDbTypesByQuery(items, 'SQL')).toEqual([items[0], items[1]]);
  });

  it('returns empty array when nothing matches', () => {
    expect(filterDbTypesByQuery(items, 'oracle')).toEqual([]);
  });
});
