import { ChevronLeft, ChevronRight } from 'lucide-react';
import { useEffect, useRef } from 'react';
import { useI18n } from '../../hooks/useI18n';
import { Button } from '../ui/Button';
import { Select } from '../ui/Select';

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

  const totalPages = Math.max(1, Math.ceil(totalRows / pageSize));
  const current = Math.min(page, totalPages - 1);
  const from = totalRows === 0 ? 0 : current * pageSize + 1;
  const to = Math.min(totalRows, (current + 1) * pageSize);
  const pageLabel = [t('pagination.page'), `${current + 1} / ${totalPages}`, t('pagination.pageOf')].filter(Boolean).join(' ');

  return (
    <div
      className="flex h-10 shrink-0 items-center justify-between gap-3 border-t border-edge bg-surface-alt px-3 text-xs text-fg-secondary"
      aria-busy={loading}
    >
      <div className="min-w-0 truncate">
        {from}-{to} / {totalRows}
      </div>
      <div className="flex items-center gap-2">
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
            disabled={loading || current >= totalPages - 1}
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
