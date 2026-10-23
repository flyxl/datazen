import { Plus, RefreshCw } from 'lucide-react';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';
import {
  TableFilterToolbar,
  type NamedViewMenuConfig,
} from './TableFilterToolbar';
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
  /**
   * The saved views of this table.
   *
   * Absent when this backend cannot serve them — a browser build has no host
   * store, so the entry is hidden rather than offered and refused later.
   */
  namedView?: NamedViewMenuConfig | null;
}

/**
 * The panel toolbar: refresh, insert row and the filter half.
 *
 * The filter half — the manual filter entry, the saved views and the
 * quick-filter box — lives in `TableFilterToolbar`; this component keeps the
 * two buttons that are about the table rather than about its filters.
 *
 * Every `data-testid` here is consumed by the E2E specs, so it must not
 * change.
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
  namedView,
}: TableToolbarProps) {
  const { t } = useI18n();

  return (
    <TableFilterToolbar
      panelId={panelId}
      dbSessionId={dbSessionId}
      database={database}
      tableName={tableName}
      columns={columns}
      loading={loading}
      pendingBusy={pendingBusy}
      filterPanelOpen={filterPanelOpen}
      appliedFilterCount={appliedFilterCount}
      draftFilterCount={draftFilterCount}
      onSetFilterPanelOpen={onSetFilterPanelOpen}
      onAddFilter={onAddFilter}
      onSetFilters={onSetFilters}
      onClearFilters={onClearFilters}
      namedView={namedView}
    >
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
    </TableFilterToolbar>
  );
}
