import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { Select } from '../Select';
import type { SelectOption } from '../Select';

afterEach(cleanup);

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

  it('keeps a compact trigger while giving the option list a readable width', () => {
    render(
      <Select
        value="hive"
        options={options}
        onChange={vi.fn()}
        searchable
        fitContent
        listMinWidth={176}
      />,
    );

    const trigger = screen.getByRole('textbox').parentElement;
    expect(trigger).not.toBeNull();
    vi.spyOn(trigger!, 'getBoundingClientRect').mockReturnValue({
      top: 20,
      right: 120,
      bottom: 44,
      left: 20,
      width: 100,
      height: 24,
      x: 20,
      y: 20,
      toJSON: () => ({}),
    });

    fireEvent.focus(screen.getByRole('textbox'));

    const list = document.getElementById('dz-select-listbox');
    expect(list).toBeInTheDocument();
    expect(list).toHaveStyle({ width: '176px' });
  });
});
