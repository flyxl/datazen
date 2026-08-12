import { describe, expect, it, vi, afterEach } from 'vitest';
import { render, fireEvent, cleanup, screen } from '@testing-library/react';
import { FilterEditor } from '../FilterEditor';
import type { FilterCondition } from '../../types';

vi.mock('../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../ui/Select', () => ({
  Select: ({
    value,
    options,
    onChange,
  }: {
    value: string;
    options: { value: string; label: string }[];
    onChange: (v: string) => void;
  }) => (
    <select value={value} onChange={(e) => onChange(e.target.value)}>
      {options.map((o) => (
        <option key={o.value} value={o.value}>
          {o.label}
        </option>
      ))}
    </select>
  ),
}));

afterEach(cleanup);

const columns = [{ name: 'id' }, { name: 'name' }];

describe('FilterEditor', () => {
  it('toggles AND/OR logic and adds a default filter', () => {
    const onLogicChange = vi.fn();
    const onAdd = vi.fn();
    render(
      <FilterEditor
        columns={columns}
        filters={[]}
        logic="and"
        onLogicChange={onLogicChange}
        onChange={vi.fn()}
        onAdd={onAdd}
        onRemove={vi.fn()}
        onClear={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByText('filter.or'));
    expect(onLogicChange).toHaveBeenCalledWith('or');
    fireEvent.click(screen.getByText('filter.and'));
    expect(onLogicChange).toHaveBeenCalledWith('and');

    fireEvent.click(screen.getByText('filter.add'));
    expect(onAdd).toHaveBeenCalledWith({ column: 'id', operator: 'eq', value: '' });
  });

  it('edits, clears, and removes filter rows', () => {
    const filters: FilterCondition[] = [
      { column: 'id', operator: 'eq', value: '1' },
      { column: 'name', operator: 'isNull', value: null },
    ];
    const onChange = vi.fn();
    const onRemove = vi.fn();
    const onClear = vi.fn();
    render(
      <FilterEditor
        columns={columns}
        filters={filters}
        logic="or"
        onLogicChange={vi.fn()}
        onChange={onChange}
        onAdd={vi.fn()}
        onRemove={onRemove}
        onClear={onClear}
      />,
    );

    fireEvent.click(screen.getByText('filter.clear'));
    expect(onClear).toHaveBeenCalled();

    const selects = screen.getAllByRole('combobox');
    fireEvent.change(selects[0], { target: { value: 'name' } });
    expect(onChange).toHaveBeenCalledWith(0, expect.objectContaining({ column: 'name' }));
    fireEvent.change(selects[1], { target: { value: 'like' } });
    expect(onChange).toHaveBeenCalledWith(0, expect.objectContaining({ operator: 'like' }));

    fireEvent.change(screen.getByPlaceholderText('filter.value'), { target: { value: 'alice' } });
    expect(onChange).toHaveBeenCalledWith(0, expect.objectContaining({ value: 'alice' }));

    fireEvent.click(screen.getAllByLabelText('filter.remove')[1]);
    expect(onRemove).toHaveBeenCalledWith(1);
  });

  it('disables add when there are no columns', () => {
    render(
      <FilterEditor
        columns={[]}
        filters={[]}
        logic="and"
        onLogicChange={vi.fn()}
        onChange={vi.fn()}
        onAdd={vi.fn()}
        onRemove={vi.fn()}
        onClear={vi.fn()}
      />,
    );
    expect(screen.getByText('filter.add').closest('button')).toBeDisabled();
  });
});
