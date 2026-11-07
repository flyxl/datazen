/**
 * What the pager says about the paging of the page on screen.
 *
 * The assertions are on **i18n keys and `data-*`, never on rendered
 * wording**: a test that pins the English sentence turns every copy edit
 * into a red test, and turns a red test into "update the string" instead of
 * "something broke".
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, render } from '@testing-library/react';

import { Pagination, pagingHintKey } from '../Pagination';
import type { PagingHint } from '../Pagination';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string, params?: Record<string, unknown>) => `${key}${params ? JSON.stringify(params) : ''}` }),
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

function renderPagination(pagingHint?: PagingHint) {
  return render(
    <Pagination
      page={2}
      pageSize={50}
      totalRows={1234}
      onPageChange={vi.fn()}
      onPageSizeChange={vi.fn()}
      onCountExact={vi.fn()}
      pagingHint={pagingHint}
    />,
  );
}

describe('the paging hint', () => {
  it('renders_degrade_hint', () => {
    const { getByTestId } = renderPagination({
      reason: 'noStableOrder',
      driftSuspected: false,
      mode: 'offset',
    });
    const hint = getByTestId('pagination-paging-reason');
    expect(hint.getAttribute('data-i18n-key')).toBe('pagination.hint.noStableOrder');
  });

  it('renders nothing about paging when there is nothing to say', () => {
    const { queryByTestId } = renderPagination({
      reason: null,
      driftSuspected: false,
      mode: 'keyset',
    });
    expect(queryByTestId('pagination-paging-reason')).toBeNull();
    expect(queryByTestId('pagination-paging-drift')).toBeNull();
  });

  it('marks the fast-paging pager without a reason', () => {
    const { getByTestId, queryByTestId } = renderPagination({
      reason: null,
      driftSuspected: false,
      mode: 'keyset',
    });
    expect(getByTestId('pagination-paging-mode').getAttribute('data-i18n-key')).toBe(
      'pagination.mode.keyset',
    );
    expect(queryByTestId('pagination-paging-mode')).not.toBeNull();
  });

  it('says_nothing_about_a_database_that_cannot_seek', () => {
    // The spec: a database without the capability shows **no** keyset
    // entry. A standing note in the footer of every table of every such
    // database is exactly what must not happen.
    const { queryByTestId } = renderPagination({
      reason: 'driverUnsupported',
      driftSuspected: false,
      mode: 'offset',
    });
    expect(queryByTestId('pagination-paging-mode')).toBeNull();
    expect(queryByTestId('pagination-paging-reason')).toBeNull();
  });

  it('labels_a_degraded_pager_as_offset_paging', () => {
    const { getByTestId, queryByTestId } = renderPagination({
      reason: 'noStableOrder',
      driftSuspected: false,
      mode: 'offset',
    });
    // A degraded pager says so **and** says why.
    expect(getByTestId('pagination-paging-mode').getAttribute('data-i18n-key')).toBe(
      'pagination.mode.offset',
    );
    expect(queryByTestId('pagination-paging-reason')).not.toBeNull();
  });

  it('shows_the_degradation_even_when_the_row_count_is_unknown', () => {
    // The count is unknown: the pager cannot tell that there is more than
    // one page, and the degradation must not disappear with it.
    const { queryByTestId } = render(
      <Pagination
        page={0}
        pageSize={50}
        totalRows={0}
        totalRowsKind="unsupported"
        onPageChange={vi.fn()}
        onPageSizeChange={vi.fn()}
        onCountExact={vi.fn()}
        pagingHint={{ reason: 'nullableKeyColumn', driftSuspected: false, mode: 'offset' }}
      />,
    );
    expect(queryByTestId('pagination-paging-reason')).not.toBeNull();
  });

  it('renders_drift_indicator', () => {
    const { getByTestId } = renderPagination({
      reason: null,
      driftSuspected: true,
      mode: 'keyset',
    });
    expect(getByTestId('pagination-paging-drift').getAttribute('data-i18n-key')).toBe(
      'pagination.hint.drift',
    );
  });

  it('passes the page through to the re-anchoring message', () => {
    const { getByTestId } = renderPagination({
      reason: 'notAdjacent',
      driftSuspected: false,
      mode: 'offset',
    });
    const hint = getByTestId('pagination-paging-reason');
    expect(hint.getAttribute('data-i18n-key')).toBe('pagination.hint.jumpReanchored');
    // `page` is 2, so the message names page 3 — the page the user is on.
    expect(hint.textContent).toContain('"page":3');
  });

  it('maps every reason to a key of its own', () => {
    const reasons = [
      'driverUnsupported',
      'noStableOrder',
      'nullableKeyColumn',
      'nullBoundary',
      'predicateUnavailable',
      'noPaginationClause',
      'staleCursor',
      'notAdjacent',
    ] as const;
    const keys = reasons.map(pagingHintKey);
    expect(new Set(keys).size).toBe(reasons.length);
    expect(pagingHintKey('notAdjacent')).toBe('pagination.hint.jumpReanchored');
  });
});
