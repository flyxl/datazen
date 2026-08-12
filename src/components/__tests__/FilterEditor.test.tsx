import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { render, fireEvent, cleanup, screen, act } from '@testing-library/react';
import type { ComponentProps } from 'react';
import { FILTER_VALUE_DEBOUNCE_MS, FilterEditor } from '../FilterEditor';
import type { FilterCondition } from '../../types';

vi.mock('../../hooks/useI18n', () => ({
  useI18n: () => ({
    t: (key: string) => key,
  }),
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

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

const columns = [{ name: 'id' }, { name: 'name' }];

function renderEditor(overrides: Partial<ComponentProps<typeof FilterEditor>> = {}) {
  const props = {
    columns,
    appliedFilters: [] as FilterCondition[],
    appliedLogic: 'and' as const,
    draftFilters: [] as FilterCondition[],
    draftLogic: 'and' as const,
    open: true,
    onOpenChange: vi.fn(),
    onLogicChange: vi.fn(),
    onChange: vi.fn(),
    onAdd: vi.fn(),
    onRemove: vi.fn(),
    onApply: vi.fn(),
    onClear: vi.fn(),
    ...overrides,
  };
  render(<FilterEditor {...props} />);
  return props;
}

describe('FilterEditor', () => {
  it('returns null when collapsed with no filters', () => {
    const { container } = render(
      <FilterEditor
        columns={columns}
        appliedFilters={[]}
        appliedLogic="and"
        draftFilters={[]}
        draftLogic="and"
        open={false}
        onOpenChange={vi.fn()}
        onLogicChange={vi.fn()}
        onChange={vi.fn()}
        onAdd={vi.fn()}
        onRemove={vi.fn()}
        onApply={vi.fn()}
        onClear={vi.fn()}
      />,
    );
    expect(container.firstChild).toBeNull();
  });

  it('toggles AND/OR and adds a default filter in the open panel', () => {
    const props = renderEditor({
      draftFilters: [{ column: 'id', operator: 'eq', value: '' }],
    });

    fireEvent.click(screen.getByText('filter.or'));
    expect(props.onLogicChange).toHaveBeenCalledWith('or');
    fireEvent.click(screen.getByText('filter.and'));
    expect(props.onLogicChange).toHaveBeenCalledWith('and');

    fireEvent.click(screen.getAllByText('filter.add')[0]);
    expect(props.onAdd).toHaveBeenCalledWith({ column: 'id', operator: 'eq', value: '' });
  });

  it('Apply is disabled until draft differs from applied', () => {
    const filter: FilterCondition = { column: 'name', operator: 'eq', value: 'a' };
    const props = renderEditor({
      appliedFilters: [filter],
      draftFilters: [filter],
    });
    expect(screen.getByText('filter.apply').closest('button')).toBeDisabled();

    cleanup();
    const dirty = renderEditor({
      appliedFilters: [filter],
      draftFilters: [{ ...filter, value: 'b' }],
    });
    fireEvent.click(screen.getByText('filter.apply'));
    expect(dirty.onApply).toHaveBeenCalled();
  });

  it('shows complete filters as chips and expands on click', () => {
    const filters: FilterCondition[] = [
      { column: 'id', operator: 'eq', value: '1' },
      { column: 'name', operator: 'isNull', value: null },
    ];
    const props = renderEditor({
      appliedFilters: filters,
      draftFilters: filters,
      draftLogic: 'or',
    });

    // Complete → chips (no comboboxes yet). Summary bar also shows the same text.
    expect(screen.queryAllByRole('combobox')).toHaveLength(0);
    expect(screen.getAllByText(/id filter\.eq 1/).length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText(/name filter\.isNull/).length).toBeGreaterThanOrEqual(1);

    fireEvent.click(screen.getByText('filter.clear'));
    expect(props.onClear).toHaveBeenCalled();

    // Click the chip label (not the summary) — prefer the button with edit title.
    fireEvent.click(screen.getAllByTitle('filter.editCondition')[0]);
    expect(screen.getAllByRole('combobox').length).toBeGreaterThanOrEqual(2);

    const valueInput = screen.getByPlaceholderText('filter.value');
    fireEvent.change(valueInput, { target: { value: 'alice' } });
    act(() => {
      vi.advanceTimersByTime(FILTER_VALUE_DEBOUNCE_MS);
    });
    expect(props.onChange).toHaveBeenCalledWith(0, expect.objectContaining({ value: 'alice' }));

    fireEvent.click(screen.getAllByLabelText('filter.remove')[1]);
    expect(props.onRemove).toHaveBeenCalledWith(1);
  });

  it('keeps incomplete filters in editor form', () => {
    renderEditor({
      draftFilters: [{ column: 'name', operator: 'eq', value: '' }],
    });
    expect(screen.getByPlaceholderText('filter.value')).toBeInTheDocument();
    expect(screen.getAllByRole('combobox')).toHaveLength(2);
  });

  it('does not commit filter value while IME is composing', () => {
    const props = renderEditor({
      draftFilters: [{ column: 'name', operator: 'eq', value: '' }],
    });

    const valueInput = screen.getByPlaceholderText('filter.value');
    fireEvent.compositionStart(valueInput);
    fireEvent.change(valueInput, { target: { value: 'huan' } });
    act(() => {
      vi.advanceTimersByTime(FILTER_VALUE_DEBOUNCE_MS * 2);
    });
    expect(props.onChange).not.toHaveBeenCalled();

    fireEvent.change(valueInput, { target: { value: '欢' } });
    fireEvent.compositionEnd(valueInput);
    act(() => {
      vi.advanceTimersByTime(FILTER_VALUE_DEBOUNCE_MS);
    });
    expect(props.onChange).toHaveBeenCalledWith(0, expect.objectContaining({ value: '欢' }));
  });

  it('shows applied summary when collapsed', () => {
    renderEditor({
      open: false,
      appliedFilters: [
        { column: 'region', operator: 'eq', value: '华北' },
        { column: 'category', operator: 'eq', value: '家电' },
      ],
      appliedLogic: 'and',
      draftFilters: [
        { column: 'region', operator: 'eq', value: '华北' },
        { column: 'category', operator: 'eq', value: '家电' },
      ],
    });
    expect(screen.getByText(/region filter\.eq 华北/)).toBeInTheDocument();
    expect(screen.queryByText('filter.apply')).not.toBeInTheDocument();
  });

  it('renders complete filters as compact chips by default', () => {
    const filters: FilterCondition[] = [
      { column: 'a', operator: 'eq', value: '1' },
      { column: 'b', operator: 'eq', value: '2' },
      { column: 'c', operator: 'eq', value: '3' },
      { column: 'd', operator: 'eq', value: '4' },
    ];
    renderEditor({ draftFilters: filters });
    expect(screen.queryAllByRole('combobox')).toHaveLength(0);
    expect(screen.getByText(/a filter\.eq 1/)).toBeInTheDocument();
    expect(screen.getByText(/d filter\.eq 4/)).toBeInTheDocument();
  });

  it('collapses to chip when a complete filter loses focus', () => {
    renderEditor({
      draftFilters: [{ column: 'name', operator: 'eq', value: 'alice' }],
    });
    // Fresh mount with a complete filter starts as chip (editingIndex null).
    expect(screen.queryAllByRole('combobox')).toHaveLength(0);

    fireEvent.click(screen.getAllByTitle('filter.editCondition')[0]);
    expect(screen.getAllByRole('combobox').length).toBeGreaterThanOrEqual(2);

    const valueInput = screen.getByPlaceholderText('filter.value');
    fireEvent.blur(valueInput, { relatedTarget: document.body });
    act(() => {
      vi.advanceTimersByTime(20);
    });
    expect(screen.queryAllByRole('combobox')).toHaveLength(0);
    expect(screen.getAllByTitle('filter.editCondition').length).toBeGreaterThanOrEqual(1);
  });

  it('does not collapse incomplete filter on blur', () => {
    renderEditor({
      draftFilters: [{ column: 'name', operator: 'eq', value: '' }],
    });
    const valueInput = screen.getByPlaceholderText('filter.value');
    fireEvent.blur(valueInput, { relatedTarget: document.body });
    act(() => {
      vi.advanceTimersByTime(20);
    });
    expect(screen.getByPlaceholderText('filter.value')).toBeInTheDocument();
  });

  it('collapsed summary bar can add a filter and reopen the panel', () => {
    const props = renderEditor({
      open: false,
      appliedFilters: [{ column: 'id', operator: 'eq', value: '1' }],
      draftFilters: [{ column: 'id', operator: 'eq', value: '1' }],
    });
    fireEvent.click(screen.getByText('filter.add'));
    expect(props.onOpenChange).toHaveBeenCalledWith(true);
    expect(props.onAdd).toHaveBeenCalledWith({ column: 'id', operator: 'eq', value: '' });
  });

  it('collapse button closes the open panel', () => {
    const props = renderEditor({
      draftFilters: [{ column: 'id', operator: 'eq', value: '' }],
    });
    fireEvent.click(screen.getByTestId('filter-collapse'));
    expect(props.onOpenChange).toHaveBeenCalledWith(false);
  });

  it('summary toggle flips open state', () => {
    const props = renderEditor({
      open: true,
      draftFilters: [{ column: 'id', operator: 'eq', value: '' }],
    });
    fireEvent.click(screen.getByTestId('filter-summary-toggle'));
    expect(props.onOpenChange).toHaveBeenCalledWith(false);
  });

  it('clamps editing index when draft filters shrink', () => {
    const { rerender } = render(
      <FilterEditor
        columns={columns}
        appliedFilters={[]}
        appliedLogic="and"
        draftFilters={[
          { column: 'id', operator: 'eq', value: '' },
          { column: 'name', operator: 'eq', value: '' },
        ]}
        draftLogic="and"
        open
        onOpenChange={vi.fn()}
        onLogicChange={vi.fn()}
        onChange={vi.fn()}
        onAdd={vi.fn()}
        onRemove={vi.fn()}
        onApply={vi.fn()}
        onClear={vi.fn()}
      />,
    );
    expect(screen.getAllByPlaceholderText('filter.value').length).toBeGreaterThanOrEqual(1);

    rerender(
      <FilterEditor
        columns={columns}
        appliedFilters={[]}
        appliedLogic="and"
        draftFilters={[{ column: 'id', operator: 'eq', value: '' }]}
        draftLogic="and"
        open
        onOpenChange={vi.fn()}
        onLogicChange={vi.fn()}
        onChange={vi.fn()}
        onAdd={vi.fn()}
        onRemove={vi.fn()}
        onApply={vi.fn()}
        onClear={vi.fn()}
      />,
    );
    expect(screen.getByPlaceholderText('filter.value')).toBeInTheDocument();
  });

  it('shows unapplied badge when draft differs from applied', () => {
    renderEditor({
      appliedFilters: [],
      draftFilters: [{ column: 'id', operator: 'eq', value: '1' }],
    });
    expect(screen.getByText('filter.unapplied')).toBeInTheDocument();
  });

  it('Enter on value commits and can collapse complete filter', () => {
    const props = renderEditor({
      draftFilters: [{ column: 'name', operator: 'eq', value: '' }],
    });
    const valueInput = screen.getByPlaceholderText('filter.value');
    fireEvent.change(valueInput, { target: { value: 'bob' } });
    fireEvent.keyDown(valueInput, { key: 'Enter' });
    expect(props.onChange).toHaveBeenCalledWith(0, expect.objectContaining({ value: 'bob' }));
  });

  it('selecting isNull operator collapses the condition editor', () => {
    renderEditor({
      draftFilters: [{ column: 'name', operator: 'eq', value: 'x' }],
    });
    fireEvent.click(screen.getAllByTitle('filter.editCondition')[0]);
    const selects = screen.getAllByRole('combobox');
    // column + operator
    fireEvent.change(selects[1], { target: { value: 'isNull' } });
    expect(screen.queryByPlaceholderText('filter.value')).not.toBeInTheDocument();
  });
});
