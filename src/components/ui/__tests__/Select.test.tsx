import { describe, expect, it } from 'vitest';
import type { SelectOption } from '../Select';

function filterOptions(options: readonly SelectOption[], query: string): SelectOption[] {
  const q = query.trim().toLowerCase();
  if (!q) return [...options];
  return options.filter(
    (o) => o.label.toLowerCase().includes(q) || o.value.toLowerCase().includes(q),
  );
}

describe('Select filter', () => {
  const options: SelectOption[] = [
    { value: 'hive', label: 'hive' },
    { value: 'snap', label: 'snap' },
    { value: '558:presto_afi_data', label: 'presto_afi_data' },
  ];

  it('matches label and value substrings case-insensitively', () => {
    expect(filterOptions(options, 'HIVE').map((o) => o.value)).toEqual(['hive']);
    expect(filterOptions(options, 'presto').map((o) => o.value)).toEqual(['558:presto_afi_data']);
    expect(filterOptions(options, '558').map((o) => o.value)).toEqual(['558:presto_afi_data']);
  });

  it('returns all options when query is empty', () => {
    expect(filterOptions(options, '')).toHaveLength(3);
  });
});
