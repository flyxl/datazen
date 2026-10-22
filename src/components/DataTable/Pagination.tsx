import { ChevronLeft, ChevronRight } from 'lucide-react';
import { useEffect, useRef, type ReactNode } from 'react';
import { useI18n } from '../../hooks/useI18n';
import { Button } from '../ui/Button';
import { Select } from '../ui/Select';
import { canPageForward, formatRowCount, totalRowsView } from '../../lib/totalRowsDisplay';
import type { CountStrategy, KeysetUnavailableReason, PagingMode } from '../../types';

export interface PaginationState {
  page: number;
  pageSize: number;
  totalRows: number;
}

export type PaginationAction =
  | { type: 'pageChanged'; page: number }
  | { type: 'pageSizeChanged'; pageSize: number }
  | { type: 'filterChanged' };

/** Pure state transition shared by page wiring and tests. */
export function paginationReducer(
  state: PaginationState,
  action: PaginationAction,
): PaginationState {
  switch (action.type) {
    case 'pageChanged': {
      if (!Number.isFinite(action.page)) return state;
      return { ...state, page: Math.max(0, Math.trunc(action.page)) };
    }
    case 'pageSizeChanged': {
      if (!Number.isFinite(action.pageSize)) return state;
      const pageSize = Math.trunc(action.pageSize);
      return pageSize > 0 ? { ...state, page: 0, pageSize } : state;
    }
    case 'filterChanged':
      return { ...state, page: 0 };
  }
}

/** Reset only the page when filters change; total rows and page size are retained. */
export function resetPageOnFilterChange(state: PaginationState): PaginationState {
  return paginationReducer(state, { type: 'filterChanged' });
}

/**
 * What the grid says about the paging of the page on screen.
 *
 * The reason is a **degradation**, not an error: the page was read by
 * offset, and this says why. It is the back end's reason — the front end
 * does not second-guess it.
 */
export interface PagingHint {
  /** Why the page was not read by seek, when it was not. */
  reason: KeysetUnavailableReason | null;
  /** Rows were deleted while the user was paging. */
  driftSuspected: boolean;
  /** The strategy the back end reported. */
  mode: PagingMode;
}

/**
 * The i18n key of a degradation reason.
 *
 * `notAdjacent` is not a reason but a consequence — the grid restarted from
 * the page it is on — so it has a wording of its own.
 */
export function pagingHintKey(reason: KeysetUnavailableReason): string {
  return reason === 'notAdjacent' ? 'pagination.hint.jumpReanchored' : `pagination.hint.${reason}`;
}

/**
 * Whether the pager says anything at all about a database that cannot seek.
 *
 * A database that does not support fast paging is read exactly as it was
 * before the contract existed, and the grid shows **nothing** about it:
 * there is no keyset entry to show, and a standing note would be a new
 * thing in the footer of every table of every such database. The reason
 * itself is still in the answer — the host reports the truth — it is just
 * not rendered.
 */
export function showPagingReason(reason: KeysetUnavailableReason): boolean {
  return reason !== 'driverUnsupported';
}

/**
 * Whether the pager shows the strategy it read the page with.
 *
 * "Fast paging" is worth saying when the seek is in force **and** there is
 * more than one page to walk — a table that fits on one screen is never
 * paged, and saying "fast paging" about it is noise.
 *
 * "Offset paging" is worth saying only when the offset was a
 * **fallback**: a driver that simply cannot seek is not a fallback, it is
 * how that database reads, and saying so on every page would be noise.
 */
export function showPagingLabel(hint: PagingHint, hasMoreThanOnePage: boolean): boolean {
  if (hint.reason !== null && showPagingReason(hint.reason)) {
    // A fallback happened: the reason is shown, and the strategy belongs
    // with it.
    return true;
  }
  return hint.mode === 'keyset' && hasMoreThanOnePage;
}

/** The i18n key of the strategy label. */
export function pagingLabelKey(hint: PagingHint): string {
  return hint.mode === 'keyset' ? 'pagination.mode.keyset' : 'pagination.mode.offset';
}

export interface PaginationProps {
  page: number;
  pageSize: number;
  totalRows: number;
  onPageChange: (page: number) => void;
  onPageSizeChange: (size: number) => void;
  /** Caller increments this when the applied structured filter changes. */
  filterRevision?: string | number;
  /** Fires for every filter revision; the caller owns request invalidation. */
  onPageReset?: () => void;
  /** Disables paging mutations while a table request is in flight. */
  loading?: boolean;
  /**
   * How much `totalRows` may be trusted.
   *
   * A missing kind means "exact": a back end that does not know about the
   * three states counted exactly. The estimated kind is the only one that
   * changes the *shape* of the row count — an estimate is never rendered in
   * the shape of a fact.
   */
  totalRowsKind?: CountStrategy | null;
  /**
   * The rows on the current page. In the unknown state this is the only
   * evidence that more rows may follow: a full page says "there may be more",
   * a half-filled one says "this is the end of the data as far as anybody
   * knows".
   */
  rowCount?: number;
  /** "There may be more rows". Defaults to what the row count says. */
  hasMore?: boolean;
  /** Ask for the exact count. Offered in the estimated state only. */
  onCountExact?: () => void;
  /** An exact count is being fetched right now. */
  counting?: boolean;
  /** No order is in force: rows across pages may repeat or be skipped. */
  unstablePaging?: boolean;
  /**
   * The user asked for an estimate and this database cannot give one.
   *
   * The front end knows this on its own — the driver declared it, or the
   * answer came back without an estimate — so the grid says so directly. It
   * is **not** an error: nothing failed, the database simply has no estimate
   * to give, and the read fell back to counting.
   */
  estimateUnavailable?: boolean;
  /** Extra controls rendered next to the pager (the panel's order override). */
  footerExtras?: ReactNode;
  /** What the back end said about the paging of this page. */
  pagingHint?: PagingHint;
}

export function Pagination({
  page,
  pageSize,
  totalRows,
  onPageChange,
  onPageSizeChange,
  filterRevision,
  onPageReset,
  loading = false,
  totalRowsKind,
  rowCount,
  hasMore,
  onCountExact,
  counting = false,
  unstablePaging = false,
  estimateUnavailable = false,
  footerExtras,
  pagingHint,
}: PaginationProps) {
  const { t } = useI18n();
  const previousFilterRevision = useRef<{ initialized: boolean; value?: string | number }>({
    initialized: false,
  });

  useEffect(() => {
    const previous = previousFilterRevision.current;
    if (!previous.initialized) {
      previous.initialized = true;
      previous.value = filterRevision;
      return;
    }
    if (Object.is(previous.value, filterRevision)) return;

    previous.value = filterRevision;
    onPageReset?.();
    if (page !== 0) onPageChange(0);
  }, [filterRevision, onPageChange, onPageReset, page]);

  // The three states of a row count. The unknown state has no number at all,
  // so it has no number of pages either: a total of zero would say "one page"
  // and pin the user to the first page forever.
  const view = totalRowsView(totalRowsKind, totalRows);
  const rowsOnPage = rowCount ?? 0;
  const rowsKnown = rowCount !== undefined;
  const totalPages = view.kind === 'unknown' ? 0 : Math.max(1, Math.ceil(view.value / pageSize));

  /**
   * The page the grid shows.
   *
   * Only the **exact** total may clamp it: the total is a fact, so a page
   * beyond the end belongs to the last one. An estimate is a bound — it may
   * be smaller than the real row count — and clamping the user's page to it
   * would trap them on the page they are already on (and send them
   * *backwards* from a later one). The estimate therefore never moves the
   * user: the page that was asked for is the page that is shown.
   */
  const current = view.kind === 'exact' ? Math.min(page, totalPages - 1) : page;

  /**
   * The rows of the current page.
   *
   * The exact state derives the range from the total. The estimated state
   * cannot: the estimate may be smaller than the data, and a range built
   * from it would read "101-60". The rows that are actually here are the
   * honest answer; the estimate only ever names the total.
   */
  let from: number;
  let to: number;
  if (view.kind === 'unknown') {
    from = rowsOnPage > 0 ? current * pageSize + 1 : 0;
    to = current * pageSize + rowsOnPage;
  } else if (view.kind === 'estimated' && rowsKnown) {
    from = rowsOnPage > 0 ? current * pageSize + 1 : 0;
    to = current * pageSize + rowsOnPage;
  } else {
    from = view.value === 0 ? 0 : current * pageSize + 1;
    to = Math.min(view.value, (current + 1) * pageSize);
  }

  // The estimate is a bound, not a fact: it is rendered with the "about" sign
  // and never in the shape of an exact count.
  const rowCountText =
    view.kind === 'exact'
      ? `${from}-${to} / ${formatRowCount(view.value)}`
      : view.kind === 'estimated'
        ? `${from}-${to} / ${t('pagination.rowCountEstimated', {
            count: formatRowCount(view.value),
          })}`
        : `${from}-${to} / ${t('pagination.rowCountUnknown')}`;

  const estimateTooltip =
    view.kind === 'estimated' ? t('pagination.estimateTooltip') : undefined;

  // The estimate may be smaller than the real count, so the user can walk past
  // the end of the data. Say so instead of showing an empty grid.
  const estimateOvershot = view.kind === 'estimated' && rowsOnPage === 0 && page > 0;

  // An exact total closes the grid on its own; an estimate never does (it may
  // be too small), and the unknown state only trusts a full page.
  const canGoForward =
    view.kind === 'exact'
      ? current < totalPages - 1
      : (hasMore ?? canPageForward(view, rowsOnPage, pageSize));

  const pageLabel =
    view.kind === 'unknown'
      ? t('pagination.pageOnly', { page: current + 1 })
      : [t('pagination.page'), `${current + 1} / ${totalPages}`, t('pagination.pageOf')]
          .filter(Boolean)
          .join(' ');

  return (
    <div
      className="flex h-10 shrink-0 items-center justify-between gap-3 border-t border-edge bg-surface-alt px-3 text-xs text-fg-secondary"
      aria-busy={loading}
    >
      <div className="flex min-w-0 items-center gap-2">
        <span
          className="shrink-0"
          title={estimateTooltip}
          aria-label={estimateTooltip}
          data-testid="pagination-row-count"
        >
          {rowCountText}
        </span>
        {estimateOvershot && (
          <span
            className="min-w-0 truncate text-warning"
            title={t('pagination.estimatedLessThanActual')}
            data-testid="pagination-estimate-overshot"
          >
            {t('pagination.estimatedLessThanActual')}
          </span>
        )}
        {estimateUnavailable && (
          <span
            className="min-w-0 truncate text-warning"
            title={t('pagination.estimateUnavailable')}
            data-testid="pagination-estimate-unavailable"
          >
            {t('pagination.estimateUnavailable')}
          </span>
        )}
      </div>
      <div className="flex items-center gap-2">
        {unstablePaging && (
          <span
            className="max-w-[22rem] truncate text-warning"
            title={t('pagination.unstablePaging')}
            data-testid="pagination-unstable-notice"
          >
            {t('pagination.unstablePaging')}
          </span>
        )}
        {pagingHint && showPagingLabel(pagingHint, totalRows > pageSize) && (
          <span
            className="shrink-0 text-fg-tertiary"
            title={t(pagingLabelKey(pagingHint))}
            data-testid="pagination-paging-mode"
            data-i18n-key={pagingLabelKey(pagingHint)}
          >
            {t(pagingLabelKey(pagingHint))}
          </span>
        )}
        {pagingHint?.reason && showPagingReason(pagingHint.reason) && (
          <span
            className="max-w-[22rem] truncate text-warning"
            title={t(pagingHintKey(pagingHint.reason), { page: current + 1 })}
            data-testid="pagination-paging-reason"
            data-i18n-key={pagingHintKey(pagingHint.reason)}
          >
            {t(pagingHintKey(pagingHint.reason), { page: current + 1 })}
          </span>
        )}
        {pagingHint?.driftSuspected && (
          <span
            className="max-w-[22rem] truncate text-warning"
            title={t('pagination.hint.drift')}
            data-testid="pagination-paging-drift"
            data-i18n-key="pagination.hint.drift"
          >
            {t('pagination.hint.drift')}
          </span>
        )}
        {footerExtras}
        {view.kind === 'estimated' && onCountExact && (
          <Button
            variant="secondary"
            size="sm"
            onClick={onCountExact}
            disabled={loading || counting}
            aria-label={t('pagination.countExact')}
            data-testid="pagination-count-exact"
          >
            {counting ? t('pagination.counting') : t('pagination.countExact')}
          </Button>
        )}
        <div className="hidden items-center gap-2 sm:flex">
          <span>{t('pagination.perPage')}</span>
          <Select
            className="h-8 w-[92px]"
            value={pageSize}
            options={[25, 50, 100, 200, 500].map((n) => ({ value: String(n), label: String(n) }))}
            disabled={loading}
            onChange={(v) => onPageSizeChange(Number(v))}
          />
        </div>
        <div className="flex items-center gap-1">
          <Button
            variant="secondary"
            className="h-8 w-8 px-0"
            disabled={loading || current <= 0}
            onClick={() => onPageChange(current - 1)}
            aria-label={t('pagination.prev')}
          >
            <ChevronLeft className="h-4 w-4" />
          </Button>
          <div className="w-[120px] truncate text-center tabular-nums">
            {pageLabel}
          </div>
          <Button
            variant="secondary"
            className="h-8 w-8 px-0"
            disabled={loading || !canGoForward}
            onClick={() => onPageChange(current + 1)}
            aria-label={t('pagination.next')}
          >
            <ChevronRight className="h-4 w-4" />
          </Button>
        </div>
      </div>
    </div>
  );
}
