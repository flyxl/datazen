import { describe, expect, it, vi, afterEach } from 'vitest';
import { render, fireEvent, cleanup } from '@testing-library/react';
import { Pagination, paginationReducer, resetPageOnFilterChange } from '../Pagination';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../ui/Select', () => ({
  Select: ({
    value,
    options,
    onChange,
    disabled,
  }: {
    value: string | number;
    options: { value: string; label: string }[];
    onChange: (v: string) => void;
    disabled?: boolean;
  }) => (
    <select disabled={disabled} value={String(value)} onChange={(e) => onChange(e.target.value)}>
      {options.map((o) => (
        <option key={o.value} value={o.value}>{o.label}</option>
      ))}
    </select>
  ),
}));

afterEach(cleanup);

describe('Pagination', () => {
  it('resets only page for a filter change in the pure reducer', () => {
    const state = { page: 4, pageSize: 50, totalRows: 275 };
    expect(resetPageOnFilterChange(state)).toEqual({ page: 0, pageSize: 50, totalRows: 275 });
    expect(paginationReducer(state, { type: 'filterChanged' })).toEqual({
      page: 0,
      pageSize: 50,
      totalRows: 275,
    });
  });

  it('rejects invalid page transitions in the pure reducer', () => {
    const state = { page: 2, pageSize: 50, totalRows: 275 };
    expect(paginationReducer(state, { type: 'pageChanged', page: Number.NaN })).toEqual(state);
    expect(paginationReducer(state, { type: 'pageSizeChanged', pageSize: 0.5 })).toEqual(state);
  });

  it('resets page through caller callbacks when filter revision changes', () => {
    const onPageChange = vi.fn();
    const onPageReset = vi.fn();
    const { rerender } = render(
      <Pagination
        page={3}
        pageSize={25}
        totalRows={100}
        filterRevision={1}
        onPageChange={onPageChange}
        onPageSizeChange={vi.fn()}
        onPageReset={onPageReset}
      />,
    );

    rerender(
      <Pagination
        page={3}
        pageSize={25}
        totalRows={100}
        filterRevision={2}
        onPageChange={onPageChange}
        onPageSizeChange={vi.fn()}
        onPageReset={onPageReset}
      />,
    );
    expect(onPageReset).toHaveBeenCalledTimes(1);
    expect(onPageChange).toHaveBeenCalledWith(0);
  });

  it('exposes loading state and disables paging controls', () => {
    const { container, getByLabelText } = render(
      <Pagination
        page={1}
        pageSize={25}
        totalRows={100}
        loading
        onPageChange={vi.fn()}
        onPageSizeChange={vi.fn()}
      />,
    );
    expect(container.firstElementChild).toHaveAttribute('aria-busy', 'true');
    expect(getByLabelText('pagination.prev')).toBeDisabled();
    expect(getByLabelText('pagination.next')).toBeDisabled();
    expect(container.querySelector('select')).toBeDisabled();
  });

  it('shows row range and page label', () => {
    const { getByText } = render(
      <Pagination
        page={0}
        pageSize={25}
        totalRows={100}
        onPageChange={vi.fn()}
        onPageSizeChange={vi.fn()}
      />,
    );
    expect(getByText('1-25 / 100')).toBeInTheDocument();
    expect(getByText(/pagination.page/)).toBeInTheDocument();
  });

  it('disables prev on first page and next on last page', () => {
    const { getByLabelText, rerender } = render(
      <Pagination
        page={0}
        pageSize={25}
        totalRows={100}
        onPageChange={vi.fn()}
        onPageSizeChange={vi.fn()}
      />,
    );
    expect(getByLabelText('pagination.prev')).toBeDisabled();
    expect(getByLabelText('pagination.next')).not.toBeDisabled();

    rerender(
      <Pagination
        page={3}
        pageSize={25}
        totalRows={100}
        onPageChange={vi.fn()}
        onPageSizeChange={vi.fn()}
      />,
    );
    expect(getByLabelText('pagination.prev')).not.toBeDisabled();
    expect(getByLabelText('pagination.next')).toBeDisabled();
  });

  it('calls onPageChange when navigating', () => {
    const onPageChange = vi.fn();
    const { getByLabelText } = render(
      <Pagination
        page={1}
        pageSize={25}
        totalRows={100}
        onPageChange={onPageChange}
        onPageSizeChange={vi.fn()}
      />,
    );
    fireEvent.click(getByLabelText('pagination.next'));
    expect(onPageChange).toHaveBeenCalledWith(2);

    fireEvent.click(getByLabelText('pagination.prev'));
    expect(onPageChange).toHaveBeenCalledWith(0);
  });

  it('calls onPageSizeChange when page size select changes', () => {
    const onPageSizeChange = vi.fn();
    const { container } = render(
      <Pagination
        page={0}
        pageSize={25}
        totalRows={100}
        onPageChange={vi.fn()}
        onPageSizeChange={onPageSizeChange}
      />,
    );
    const select = container.querySelector('select')!;
    fireEvent.change(select, { target: { value: '50' } });
    expect(onPageSizeChange).toHaveBeenCalledWith(50);
  });

  it('shows 0 range when totalRows is zero', () => {
    const { getByText } = render(
      <Pagination
        page={0}
        pageSize={25}
        totalRows={0}
        onPageChange={vi.fn()}
        onPageSizeChange={vi.fn()}
      />,
    );
    expect(getByText('0-0 / 0')).toBeInTheDocument();
  });
});
