import { describe, expect, it, vi, afterEach } from 'vitest';
import { render, fireEvent, cleanup } from '@testing-library/react';
import { Pagination } from '../Pagination';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../ui/Select', () => ({
  Select: ({
    value,
    options,
    onChange,
  }: {
    value: string | number;
    options: { value: string; label: string }[];
    onChange: (v: string) => void;
  }) => (
    <select value={String(value)} onChange={(e) => onChange(e.target.value)}>
      {options.map((o) => (
        <option key={o.value} value={o.value}>{o.label}</option>
      ))}
    </select>
  ),
}));

afterEach(cleanup);

describe('Pagination', () => {
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
