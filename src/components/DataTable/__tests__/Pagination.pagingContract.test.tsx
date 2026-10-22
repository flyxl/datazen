import { describe, expect, it, vi, afterEach } from 'vitest';
import { render, fireEvent, cleanup } from '@testing-library/react';
import { Pagination } from '../Pagination';

/**
 * Independent re-derivation (verifier, round 2) of the page-clamping contract.
 *
 * Written from PRD 09 §7 B5 and §4.1, not from the implementation:
 *
 *  - The **estimated** total is a bound, not a fact. It may be smaller than
 *    the real row count, so it must never move the user: the page that was
 *    asked for is the page that is shown, and "Next" is one page forward.
 *  - The **exact** total *is* a fact. A page beyond the end belongs to the
 *    last one, so the exact state still clamps. Removing that clamp would be
 *    a different defect, not a fix.
 *  - The label and the row range must read the **same** page, or the grid
 *    shows "Page 3" next to "51-60".
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

describe('round 2 · the estimated state never moves the user', () => {
  it('shows the page the user asked for, not the page the estimate allows', () => {
    // The estimate says 60 rows (2 pages); the table really has thousands.
    const { getByText } = renderPagination({
      totalRowsKind: 'estimated',
      totalRows: 60,
      page: 2,
      rowCount: 50,
    });
    expect(getByText(/Page/).textContent).toBe('Page 3 / 2');
  });

  it('sends "Next" one page forward', () => {
    const { getByLabelText, onPageChange } = renderPagination({
      totalRowsKind: 'estimated',
      totalRows: 60,
      page: 2,
      rowCount: 50,
    });
    fireEvent.click(getByLabelText('Next'));
    expect(onPageChange).toHaveBeenCalledWith(3);
    expect(onPageChange).not.toHaveBeenCalledWith(2);
  });

  it('does not send the user backwards from a page past the estimated end', () => {
    const { getByLabelText, onPageChange } = renderPagination({
      totalRowsKind: 'estimated',
      totalRows: 60,
      page: 5,
      rowCount: 50,
    });
    fireEvent.click(getByLabelText('Next'));
    expect(onPageChange).toHaveBeenCalledWith(6);
    expect(onPageChange).not.toHaveBeenCalledWith(2);
  });

  it('sends "Prev" one page backwards', () => {
    const { getByLabelText, onPageChange } = renderPagination({
      totalRowsKind: 'estimated',
      totalRows: 60,
      page: 5,
      rowCount: 50,
    });
    fireEvent.click(getByLabelText('Previous'));
    expect(onPageChange).toHaveBeenCalledWith(4);
  });

  it('reads the row range from the same page as the label', () => {
    // The label says page 3; the range must therefore be 101-150. A range
    // built from the *estimate* would read "101-60" — an impossible range.
    const { getByTestId, getByText } = renderPagination({
      totalRowsKind: 'estimated',
      totalRows: 60,
      page: 2,
      rowCount: 50,
    });
    expect(getByText(/Page/).textContent).toBe('Page 3 / 2');
    expect(getByTestId('pagination-row-count').textContent).toBe('101-150 / ≈ 60');
  });
});

describe('round 2 · the exact state still clamps — the total is a fact', () => {
  it('clamps a page beyond the end back to the last one', () => {
    // 60 rows, 50 per page ⇒ two pages. The user asked for the sixth.
    const { getByText } = renderPagination({
      totalRowsKind: 'exact',
      totalRows: 60,
      page: 5,
      rowCount: 50,
    });
    expect(getByText(/Page/).textContent).toBe('Page 2 / 2');
  });

  it('keeps the range of the clamped page', () => {
    const { getByTestId } = renderPagination({
      totalRowsKind: 'exact',
      totalRows: 60,
      page: 5,
      rowCount: 50,
    });
    expect(getByTestId('pagination-row-count').textContent).toBe('51-60 / 60');
  });

  it('does not clamp a page that exists', () => {
    const { getByText } = renderPagination({
      totalRowsKind: 'exact',
      totalRows: 1234,
      page: 5,
      rowCount: 50,
    });
    expect(getByText(/Page/).textContent).toBe('Page 6 / 25');
  });
});

describe('round 2 · the unknown state is untouched by the clamping rule', () => {
  it('shows the page number alone', () => {
    const { getByText, queryByText } = renderPagination({
      totalRowsKind: 'unsupported',
      totalRows: 0,
      page: 4,
      rowCount: 50,
    });
    expect(getByText('Page 5')).toBeInTheDocument();
    expect(queryByText(/Page 5 \//)).toBeNull();
  });

  it('keeps "Next" alive while the page is full', () => {
    const { getByLabelText } = renderPagination({
      totalRowsKind: 'unsupported',
      totalRows: 0,
      rowCount: 50,
    });
    expect(getByLabelText('Next')).not.toBeDisabled();
  });
});
