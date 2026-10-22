import { describe, expect, it, vi, afterEach } from 'vitest';
import { render, fireEvent, cleanup } from '@testing-library/react';
import { Pagination } from '../Pagination';

/**
 * An estimate that is **smaller than the real row count** (PRD 09 §7 B5).
 *
 * "估算值小于实际行数 … 这条既有行为不得引入新的死循环（用「按用户请求的 page
 * 拉数据、只用 kind 决定渲染」的方式绕开）".
 *
 * The estimate is a bound, not a fact: it may be smaller than the data, and
 * the pager must not clamp the user's page to it. Clamping did exactly that —
 * the pager showed a page the user was not on, and "Next" sent them
 * *backwards* (or nowhere at all, because the callback repeated the page they
 * were already on).
 *
 * These three cases were registered by the verifier as a known defect and are
 * kept as the permanent guard for it: the estimate never moves the user.
 */
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

function renderPagination(props: Partial<React.ComponentProps<typeof Pagination>> = {}) {
  const onPageChange = vi.fn();
  const onPageSizeChange = vi.fn();
  const onCountExact = vi.fn();
  const view = render(
    <Pagination
      page={0}
      pageSize={50}
      totalRows={1234}
      onPageChange={onPageChange}
      onPageSizeChange={onPageSizeChange}
      onCountExact={onCountExact}
      {...props}
    />,
  );
  return { ...view, onPageChange, onPageSizeChange, onCountExact };
}

describe('an estimate that is smaller than the real row count', () => {
  it('shows the page the user asked for, not the page the estimate allows', () => {
    // The estimate says 60 rows (2 pages); the table really has thousands.
    // The user is on the third page (index 2) and the page is full.
    const { getByText } = renderPagination({
      totalRowsKind: 'estimated',
      totalRows: 60,
      page: 2,
      rowCount: 50,
    });
    expect(getByText(/Page/).textContent).toBe('Page 3 / 2');
  });

  it('lets "Next" move one page forward instead of clamping to the estimate', () => {
    const { getByLabelText, onPageChange } = renderPagination({
      totalRowsKind: 'estimated',
      totalRows: 60,
      page: 2,
      rowCount: 50,
    });
    fireEvent.click(getByLabelText('Next'));
    // Clamping **used to** send the user back to page index 2 — the page they
    // were already on. The contract asks for one page forward.
    expect(onPageChange).toHaveBeenCalledWith(3);
  });

  it('does not send the user backwards from a page past the estimated end', () => {
    const { getByLabelText, onPageChange } = renderPagination({
      totalRowsKind: 'estimated',
      totalRows: 60,
      page: 5,
      rowCount: 50,
    });
    fireEvent.click(getByLabelText('Next'));
    // Clamping **used to** call onPageChange(2) — from page index 5 back to
    // page index 2.
    expect(onPageChange).toHaveBeenCalledWith(6);
  });
});
