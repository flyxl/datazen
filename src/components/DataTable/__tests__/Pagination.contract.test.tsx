import { describe, expect, it, vi, afterEach } from 'vitest';
import { render, fireEvent, cleanup } from '@testing-library/react';
import { Pagination } from '../Pagination';
import { formatRowCount, totalRowsView } from '../../../lib/totalRowsDisplay';

/**
 * Independent re-derivation of the three-state contract (PRD 09 §4.1).
 *
 * Written by the verifier, not by the implementer: every assertion here is
 * derived from the contract text, not from the implementation. The point is
 * to fail when the implementation drifts — including when it drifts in a way
 * the implementer's own tests happen to accept.
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

const TOOLTIP = 'Estimated row count — the real number may differ. Use Count for the exact value.';

describe('contract §4.1 · the exact state', () => {
  it('shows the range and the total, with no "about" sign anywhere', () => {
    const { getByTestId } = renderPagination({ totalRowsKind: 'exact', totalRows: 1234 });
    const text = getByTestId('pagination-row-count').textContent ?? '';
    expect(text).toBe('1-50 / 1,234');
    expect(text).not.toContain('≈');
    expect(text).not.toContain('NaN');
    expect(text).not.toContain('Infinity');
  });

  it('keeps the total number of pages on the right', () => {
    const { getByText } = renderPagination({ totalRowsKind: 'exact', totalRows: 1234 });
    expect(getByText('Page 1 / 25')).toBeInTheDocument();
  });

  it('has no "Count" button — an exact count cannot be improved on', () => {
    const { queryByTestId } = renderPagination({ totalRowsKind: 'exact' });
    expect(queryByTestId('pagination-count-exact')).toBeNull();
  });

  it('treats a missing kind as exact (an old back end counted exactly)', () => {
    const { getByTestId } = renderPagination({ totalRowsKind: undefined, totalRows: 1234 });
    expect(getByTestId('pagination-row-count').textContent).toBe('1-50 / 1,234');
  });
});

describe('contract §4.1 · the estimated state', () => {
  it('puts the "about" sign in the text, not in a decoration', () => {
    const { getByTestId } = renderPagination({
      totalRowsKind: 'estimated',
      totalRows: 1_200_000,
    });
    const text = getByTestId('pagination-row-count').textContent ?? '';
    expect(text).toBe('1-50 / ≈ 1.2M');
    expect(text).toContain('≈');
  });

  it('never renders an estimate in the shape of a fact', () => {
    const exact = renderPagination({ totalRowsKind: 'exact', totalRows: 1234 });
    const exactText = exact.getByTestId('pagination-row-count').textContent;
    exact.unmount();
    const estimated = renderPagination({ totalRowsKind: 'estimated', totalRows: 1234 });
    const estimatedText = estimated.getByTestId('pagination-row-count').textContent;
    expect(exactText).toBe('1-50 / 1,234');
    expect(estimatedText).toBe('1-50 / ≈ 1,234');
    expect(estimatedText).not.toBe(exactText);
  });

  it('says on the element itself that the number may be wrong', () => {
    const { getByTestId } = renderPagination({ totalRowsKind: 'estimated' });
    const el = getByTestId('pagination-row-count');
    expect(el.getAttribute('title')).toBe(TOOLTIP);
    expect(el.getAttribute('aria-label')).toBe(TOOLTIP);
  });

  it('offers the exact count and asks for it exactly once per click', () => {
    const { getByTestId, onCountExact } = renderPagination({ totalRowsKind: 'estimated' });
    fireEvent.click(getByTestId('pagination-count-exact'));
    expect(onCountExact).toHaveBeenCalledTimes(1);
  });

  it('keeps the number of pages an estimate implies', () => {
    const { getByText } = renderPagination({
      totalRowsKind: 'estimated',
      totalRows: 1_200_000,
    });
    expect(getByText('Page 1 / 24000')).toBeInTheDocument();
  });
});

describe('contract §4.1 · the unknown state', () => {
  it('says the count is unknown instead of showing a number nobody counted', () => {
    const { getByTestId } = renderPagination({
      totalRowsKind: 'unsupported',
      totalRows: 0,
      rowCount: 50,
    });
    const text = getByTestId('pagination-row-count').textContent ?? '';
    expect(text).toBe('1-50 / —');
    expect(text).not.toContain('0 rows');
    expect(text).not.toContain('NaN');
    expect(text).not.toContain('Infinity');
  });

  it('renders no total number of pages — not "Page 1 / 1"', () => {
    const { queryByText, getByText } = renderPagination({
      totalRowsKind: 'unsupported',
      totalRows: 0,
      page: 2,
      rowCount: 50,
    });
    expect(queryByText('Page 1 / 1')).toBeNull();
    expect(queryByText('Page 3 / 1')).toBeNull();
    expect(getByText('Page 3')).toBeInTheDocument();
  });

  it('never lets a number of pages leak into the unknown state', () => {
    const { getByText } = renderPagination({
      totalRowsKind: 'unsupported',
      totalRows: 0,
      page: 0,
      rowCount: 50,
    });
    const label = getByText('Page 1');
    expect(label.textContent).not.toContain('/');
  });

  it('keeps Next available while the page is full, without a hasMore hint', () => {
    // canPageForward(view, rows.length, pageSize) is the only evidence there
    // is; a full page means "there may be more".
    const { getByLabelText } = renderPagination({
      totalRowsKind: 'unsupported',
      totalRows: 0,
      rowCount: 50,
    });
    expect(getByLabelText('Next')).not.toBeDisabled();
  });

  it('stops at the end of the data when the page is not full', () => {
    const { getByLabelText } = renderPagination({
      totalRowsKind: 'unsupported',
      totalRows: 0,
      rowCount: 12,
    });
    expect(getByLabelText('Next')).toBeDisabled();
  });

  it('keeps Next available when the caller says there may be more', () => {
    const { getByLabelText } = renderPagination({
      totalRowsKind: 'unsupported',
      totalRows: 0,
      rowCount: 50,
      hasMore: true,
    });
    expect(getByLabelText('Next')).not.toBeDisabled();
  });

  it('has no "Count" button — the database cannot count', () => {
    const { queryByTestId } = renderPagination({ totalRowsKind: 'unsupported' });
    expect(queryByTestId('pagination-count-exact')).toBeNull();
  });

  it('is not "zero rows": the view carries no number at all', () => {
    expect(totalRowsView('unsupported', 1234)).toEqual({ kind: 'unknown' });
  });
});

describe('contract §4.1 constraint 1 · the tilde belongs to the text, not to the number', () => {
  it('formatRowCount itself never carries the "about" sign', () => {
    expect(formatRowCount(1_200_000)).toBe('1.2M');
    expect(formatRowCount(1_200_000)).not.toContain('≈');
    expect(formatRowCount(1234)).toBe('1,234');
    expect(formatRowCount(1234)).not.toContain('≈');
  });

  it('and the rendered text does', () => {
    const { getByTestId } = renderPagination({
      totalRowsKind: 'estimated',
      totalRows: 1_200_000,
    });
    expect(getByTestId('pagination-row-count').textContent).toContain('≈');
  });
});
