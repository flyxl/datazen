import { useCallback, useState } from 'react';
import { Filter, Plus, RefreshCw } from 'lucide-react';
import { NlFilterInput } from '../../components/ai/NlFilterInput';
import { ErrorBanner } from '../../components/ui/ErrorBanner';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';
import {
  filterExpressionToConditions,
  parseFilterForApply,
  type FilterExpression,
} from '../../lib/filterExpression';
import type { ColumnSchema, FilterCondition } from '../../types';

export interface TableToolbarProps {
  panelId: string;
  dbSessionId: string;
  database: string;
  tableName: string;
  /** Column metadata; the quick filter needs it to parse an expression. */
  columns: ColumnSchema[];
  /** A page load is in flight. */
  loading: boolean;
  /** Loading or previewing/committing: the write affordances are disabled. */
  pendingBusy: boolean;
  /** Read-only panels get no "insert row" button. */
  isEditable: boolean;
  /** The manual filter editor is open. */
  filterPanelOpen: boolean;
  /** Applied filters — they light the filter button up. */
  appliedFilterCount: number;
  /** Draft (not yet applied) filters — they light the filter button up too. */
  draftFilterCount: number;
  /** Re-read the current page. */
  onRefresh: () => void;
  /** Open or close the manual filter editor. */
  onSetFilterPanelOpen: (open: boolean) => void;
  /** Add a draft filter. */
  onAddFilter: (filter: FilterCondition) => void;
  /** Replace the applied filters. */
  onSetFilters: (filters: FilterCondition[], logic?: 'and' | 'or') => void;
  /** Drop every applied filter. */
  onClearFilters: () => void;
  /** Append a new-row draft. */
  onAddDraftRow: () => void;
}

/**
 * The panel toolbar: refresh, insert row, filter, the natural-language filter
 * input and the quick-filter box.
 *
 * Extracted verbatim from `TableView` (DB-06 track, GC budget) — the quick
 * filter's draft state and its two handlers moved with the markup, because
 * both of them are only ever used by this toolbar. Every `data-testid` here is
 * consumed by the E2E specs, so it must not change.
 */
export function TableToolbar({
  panelId,
  dbSessionId,
  database,
  tableName,
  columns,
  loading,
  pendingBusy,
  isEditable,
  filterPanelOpen,
  appliedFilterCount,
  draftFilterCount,
  onRefresh,
  onSetFilterPanelOpen,
  onAddFilter,
  onSetFilters,
  onClearFilters,
  onAddDraftRow,
}: TableToolbarProps) {
  const { t } = useI18n();
  const [quickFilter, setQuickFilter] = useState('');
  const [quickFilterError, setQuickFilterError] = useState<string | null>(null);

  const handleQuickFilter = useCallback(() => {
    const input = quickFilter.trim();
    if (!input) {
      setQuickFilterError(null);
      onClearFilters();
      return;
    }
    const parsed = parseFilterForApply(input, columns);
    if (!parsed.ok) {
      setQuickFilterError(t('filter.invalidExpression', { error: parsed.error }));
      return;
    }
    const logic = new Set<'and' | 'or'>();
    const collectLogic = (expression: FilterExpression) => {
      if (expression.type === 'logical') {
        logic.add(expression.operator);
        collectLogic(expression.left);
        collectLogic(expression.right);
      }
    };
    collectLogic(parsed.value.expression);
    if (logic.size > 1) {
      setQuickFilterError(t('filter.mixedLogic'));
      return;
    }
    setQuickFilterError(null);
    onSetFilters(filterExpressionToConditions(parsed.value.expression), [...logic][0] ?? 'and');
  }, [columns, onClearFilters, onSetFilters, quickFilter, t]);

  const openManualFilter = () => {
    if (filterPanelOpen) {
      onSetFilterPanelOpen(false);
      return;
    }
    onSetFilterPanelOpen(true);
    if (draftFilterCount === 0) {
      onAddFilter({
        column: columns[0]?.name ?? '',
        operator: 'eq',
        value: '',
      });
    }
  };

  const filterActive = filterPanelOpen || appliedFilterCount > 0 || draftFilterCount > 0;

  return (
    <>
      <div className="flex shrink-0 items-start gap-0.5 border-b border-edge px-2 py-0.5">
        <button
          type="button"
          onMouseDown={(e) => e.preventDefault()}
          className="mt-0 flex h-7 w-7 shrink-0 items-center justify-center rounded text-xs text-fg-muted transition-colors hover:bg-surface-alt hover:text-fg disabled:opacity-50"
          onClick={onRefresh}
          disabled={loading}
          title={t('connWin.refresh')}
          aria-label={t('connWin.refresh')}
          data-testid="table-data-refresh"
        >
          <RefreshCw className={cn('h-3.5 w-3.5', loading && 'animate-spin')} />
        </button>
        {isEditable && (
          <button
            type="button"
            onMouseDown={(e) => e.preventDefault()}
            className="mt-0 flex h-7 w-7 shrink-0 items-center justify-center rounded text-xs text-fg-muted transition-colors hover:bg-surface-alt hover:text-fg disabled:opacity-50"
            onClick={onAddDraftRow}
            disabled={loading || pendingBusy}
            title={t('tableData.insertRowHint', { shortcut: '⌘/Ctrl+I' })}
            aria-label={t('tableData.insertRow')}
            data-testid="table-insert-row"
          >
            <Plus className="h-3.5 w-3.5" />
          </button>
        )}
        <button
          type="button"
          onMouseDown={(e) => e.preventDefault()}
          className={cn(
            'mt-0 flex h-7 w-7 shrink-0 items-center justify-center rounded text-xs transition-colors hover:bg-surface-alt',
            filterActive ? 'text-accent' : 'text-fg-muted hover:text-fg',
          )}
          onClick={openManualFilter}
          disabled={loading}
          title={t('filter.filter')}
          aria-label={t('filter.filter')}
          aria-pressed={filterPanelOpen}
          data-testid="table-filter-toggle"
        >
          <Filter className="h-3.5 w-3.5" />
        </button>
        <NlFilterInput
          panelId={panelId}
          dbSessionId={dbSessionId}
          database={database}
          tableName={tableName}
        />
        <div className="ml-1 flex min-w-0 flex-1 items-center">
          <input
            type="text"
            value={quickFilter}
            onChange={(event) => {
              setQuickFilter(event.target.value);
              if (quickFilterError) setQuickFilterError(null);
            }}
            onKeyDown={(event) => {
              if (event.key === 'Enter' && !event.nativeEvent.isComposing) {
                event.preventDefault();
                handleQuickFilter();
              }
            }}
            onBlur={() => {
              if (quickFilter.trim()) handleQuickFilter();
            }}
            disabled={loading || pendingBusy}
            placeholder={t('filter.quickPlaceholder')}
            className="h-7 min-w-0 flex-1 rounded border border-edge bg-surface px-2 text-xs text-fg placeholder:text-fg-muted focus:border-accent focus:outline-none"
            aria-invalid={quickFilterError ? 'true' : undefined}
            data-testid="table-quick-filter"
          />
        </div>
      </div>
      {quickFilterError && (
        <ErrorBanner variant="strip" className="border-danger/30 py-1">
          {quickFilterError}
        </ErrorBanner>
      )}
    </>
  );
}
