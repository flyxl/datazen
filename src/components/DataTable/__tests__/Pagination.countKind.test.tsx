import { describe, expect, it, vi, afterEach } from 'vitest';
import { render, fireEvent, cleanup } from '@testing-library/react';
import { Pagination } from '../Pagination';

/**
 * The three states of a row count.
 *
 * These tests run against the **real** English dictionary: the "about" sign
 * is part of the text the user reads, and a test that mocked the dictionary
 * away would only prove that the component asked for a key — not that the
 * sign ever reaches the screen.
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

describe('Pagination · the exact state', () => {
  it('renders the range and the total without the "about" sign', () => {
    const { getByTestId } = renderPagination({ totalRowsKind: 'exact', totalRows: 1234 });
    expect(getByTestId('pagination-row-count').textContent).toBe('1-50 / 1,234');
    expect(getByTestId('pagination-row-count').textContent).not.toContain('≈');
  });

  it('offers no count button — there is nothing to improve on', () => {
    const { queryByTestId } = renderPagination({ totalRowsKind: 'exact' });
    expect(queryByTestId('pagination-count-exact')).toBeNull();
  });

  it('keeps the total number of pages', () => {
    const { getByText } = renderPagination({ totalRowsKind: 'exact', totalRows: 1234 });
    expect(getByText('Page 1 / 25')).toBeInTheDocument();
  });
});

describe('Pagination · the estimated state', () => {
  it('renders the estimate with the "about" sign', () => {
    const { getByTestId } = renderPagination({
      totalRowsKind: 'estimated',
      totalRows: 1_200_000,
    });
    expect(getByTestId('pagination-row-count').textContent).toBe('1-50 / ≈ 1.2M');
  });

  it('looks different from the exact state', () => {
    // The hard constraint of the count contract: an estimate must never be
    // rendered in the shape of a fact. Same number, different rendering.
    const exact = renderPagination({ totalRowsKind: 'exact', totalRows: 1234 });
    const exactText = exact.getByTestId('pagination-row-count').textContent;
    exact.unmount();

    const estimated = renderPagination({ totalRowsKind: 'estimated', totalRows: 1234 });
    const estimatedText = estimated.getByTestId('pagination-row-count').textContent;

    expect(exactText).toBe('1-50 / 1,234');
    expect(estimatedText).toBe('1-50 / ≈ 1,234');
    expect(estimatedText).not.toBe(exactText);
  });

  it('says that the number may be wrong', () => {
    const { getByTestId } = renderPagination({ totalRowsKind: 'estimated' });
    const rowCount = getByTestId('pagination-row-count');
    expect(rowCount.getAttribute('title')).toBe(
      'Estimated row count — the real number may differ. Use Count for the exact value.',
    );
    expect(rowCount.getAttribute('aria-label')).toBe(
      'Estimated row count — the real number may differ. Use Count for the exact value.',
    );
  });

  it('offers the exact count and asks for it when it is clicked', () => {
    const { getByTestId, onCountExact } = renderPagination({ totalRowsKind: 'estimated' });
    fireEvent.click(getByTestId('pagination-count-exact'));
    expect(onCountExact).toHaveBeenCalledTimes(1);
  });

  it('says that it is counting while the exact count is on its way', () => {
    const { getByTestId } = renderPagination({ totalRowsKind: 'estimated', counting: true });
    expect(getByTestId('pagination-count-exact').textContent).toBe('Counting…');
  });

  it('warns when the estimate was higher than the data', () => {
    const { getByTestId } = renderPagination({
      totalRowsKind: 'estimated',
      page: 3,
      rowCount: 0,
    });
    expect(getByTestId('pagination-estimate-overshot')).toBeInTheDocument();
  });

  it('never clamps the user to a page the estimate allows', () => {
    // The estimate is a bound: it may be smaller than the real row count. The
    // pager therefore shows the page the user asked for and moves them one
    // page forward — clamping the estimate used to pin them to the page they
    // were already on (or send them backwards).
    const { getByText, getByLabelText, onPageChange } = renderPagination({
      totalRowsKind: 'estimated',
      totalRows: 60,
      page: 2,
      rowCount: 50,
    });
    // The estimate implies two pages; the user is on the third.
    expect(getByText('Page 3 / 2')).toBeInTheDocument();
    fireEvent.click(getByLabelText('Next'));
    expect(onPageChange).toHaveBeenCalledWith(3);
  });

  it('still clamps an exact total, which is a fact', () => {
    // The exact state is the opposite case: the total is real, so a page
    // beyond the end belongs to the last one.
    const { getByText } = renderPagination({
      totalRowsKind: 'exact',
      totalRows: 60,
      page: 5,
      rowCount: 10,
    });
    expect(getByText('Page 2 / 2')).toBeInTheDocument();
  });

  it('says that this database cannot estimate when the user asked for one', () => {
    // The front end knows this on its own: the driver declared it, or the
    // answer came back without an estimate. It is a rendered fact, not an
    // error to classify — nothing failed, the read simply fell back.
    const { getByTestId } = renderPagination({
      totalRowsKind: 'exact',
      estimateUnavailable: true,
    });
    const notice = getByTestId('pagination-estimate-unavailable');
    expect(notice).toBeInTheDocument();
    expect(notice.textContent).toBe('This database cannot estimate row counts.');
  });

  it('says nothing about estimates when the database gave one', () => {
    const { queryByTestId } = renderPagination({
      totalRowsKind: 'estimated',
      estimateUnavailable: false,
    });
    expect(queryByTestId('pagination-estimate-unavailable')).toBeNull();
  });
});

describe('Pagination · the unknown state', () => {
  it('says that the count is unknown instead of showing zero rows', () => {
    const { getByTestId } = renderPagination({
      totalRowsKind: 'unsupported',
      totalRows: 0,
      rowCount: 50,
    });
    const rowCount = getByTestId('pagination-row-count');
    expect(rowCount.textContent).toBe('1-50 / —');
    expect(rowCount.textContent).not.toContain('0 rows');
  });

  it('shows no total number of pages', () => {
    const { queryByText, getByText } = renderPagination({
      totalRowsKind: 'unsupported',
      totalRows: 0,
      page: 2,
      rowCount: 50,
    });
    expect(queryByText('Page 1 / 1')).toBeNull();
    expect(getByText('Page 3')).toBeInTheDocument();
  });

  it('keeps the next page available while the page is full', () => {
    const { getByLabelText } = renderPagination({
      totalRowsKind: 'unsupported',
      totalRows: 0,
      rowCount: 50,
      hasMore: true,
    });
    expect(getByLabelText('Next')).not.toBeDisabled();
  });

  it('stops at the end of the data when the page is not full', () => {
    const { getByLabelText } = renderPagination({
      totalRowsKind: 'unsupported',
      totalRows: 0,
      rowCount: 12,
      hasMore: false,
    });
    expect(getByLabelText('Next')).toBeDisabled();
  });

  it('offers no count button — the database cannot count', () => {
    const { queryByTestId } = renderPagination({ totalRowsKind: 'unsupported' });
    expect(queryByTestId('pagination-count-exact')).toBeNull();
  });
});

describe('Pagination · the stability notice', () => {
  it('warns that the rows may repeat or be skipped without an order', () => {
    const { getByTestId } = renderPagination({ unstablePaging: true });
    const notice = getByTestId('pagination-unstable-notice');
    expect(notice).toBeInTheDocument();
    expect(notice.textContent).toBe(
      'No sort order is set — rows across pages may repeat or be skipped.',
    );
  });

  it('says nothing when an order is in force', () => {
    const { queryByTestId } = renderPagination({ unstablePaging: false });
    expect(queryByTestId('pagination-unstable-notice')).toBeNull();
  });
});
