import { describe, expect, it } from 'vitest';
import { sortDbTypesByPopularity } from '../databaseTypes';

describe('sortDbTypesByPopularity', () => {
  it('orders common types ahead of protocol-family injection order', () => {
    const items = [
      { value: 'questdb', label: 'QuestDB' },
      { value: 'postgresql', label: 'PostgreSQL' },
      { value: 'doris', label: 'Doris' },
      { value: 'mysql', label: 'MySQL' },
      { value: 'redis', label: 'Redis' },
    ];
    expect(sortDbTypesByPopularity(items).map((x) => x.value)).toEqual([
      'mysql',
      'postgresql',
      'redis',
      'doris',
      'questdb',
    ]);
  });

  it('places unknown ids after known popularity ranks, then by id', () => {
    const items = [
      { value: 'zzz_custom', label: 'Z' },
      { value: 'aaa_custom', label: 'A' },
      { value: 'mysql', label: 'MySQL' },
    ];
    expect(sortDbTypesByPopularity(items).map((x) => x.value)).toEqual([
      'mysql',
      'aaa_custom',
      'zzz_custom',
    ]);
  });
});
