import { useCallback, useState, type ReactNode } from 'react';
import { Filter } from 'lucide-react';
import { NlFilterInput } from '../../components/ai/NlFilterInput';
import { ErrorBanner } from '../../components/ui/ErrorBanner';
import { NamedViewMenu, type NamedViewMenuProps } from '../../components/filter/NamedViewMenu';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';
import { flattenToConnectors, parseFilterForApply } from '../../lib/filterExpression';
import type { NamedView, NamedViewScope, TableStateForView } from '../../lib/namedViews';
import type { ColumnSchema, FilterCondition } from '../../types';

/**
 * The filter half of the panel toolbar: the manual filter entry, the saved
 * views and the quick-filter box.
 *
 * It is a separate file because the toolbar grew past the point where every
 * new filter affordance could be added to it: the saved-views menu alone is a
 * state machine with six states, and it does not belong in the same file as
 * the refresh button.
 *
 * The markup is **unchanged** — `TableToolbar`'s golden file pins it byte for
 * byte — so the buttons that are not filter-related arrive as `children` and
 * are rendered first, exactly where they were.
 */

/** What the saved-views menu needs from the panel. */
export interface NamedViewMenuConfig {
  /** The scope the views belong to: connection id, database, schema, table. */
  scope: NamedViewScope;
  /** The applied state — what "Save as…" saves. */
  applied: TableStateForView;
  /** The draft state, when the user has unapplied changes. */
  draft?: { filters: FilterCondition[]; filterLogic: 'and' | 'or' } | null;
  /** Whether the draft differs from the applied state. */
  dirty?: boolean;
  /** Apply a view to the table. */
  onApply: (view: NamedView) => void;
  /** Apply the draft filters (the "Apply first" action). */
  onApplyDraft?: () => void;
  /** The three host commands the menu drives. */
  operations: NamedViewMenuProps['operations'];
  /** The two file commands the export and the import drive. */
  files: NamedViewMenuProps['files'];
}

export interface TableFilterToolbarProps {
  /** The buttons that are not filter-related; they come first in the row. */
  children?: ReactNode;
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
  /** The manual filter editor is open. */
  filterPanelOpen: boolean;
  /** Applied filters — they light the filter button up. */
  appliedFilterCount: number;
  /** Draft (not yet applied) filters — they light the filter button up too. */
  draftFilterCount: number;
  /** Open or close the manual filter editor. */
  onSetFilterPanelOpen: (open: boolean) => void;
  /** Add a draft filter. */
  onAddFilter: (filter: FilterCondition) => void;
  /** Replace the applied filters. */
  onSetFilters: (filters: FilterCondition[], logic?: 'and' | 'or') => void;
  /** Drop every applied filter. */
  onClearFilters: () => void;
  /**
   * The saved views of this table.
   *
   * Absent when this backend cannot serve them: a browser build has no host
   * store, so the entry is **hidden** rather than offered and refused later.
   */
  namedView?: NamedViewMenuConfig | null;
}

export function TableFilterToolbar({
  children,
  panelId,
  dbSessionId,
  database,
  tableName,
  columns,
  loading,
  pendingBusy,
  filterPanelOpen,
  appliedFilterCount,
  draftFilterCount,
  onSetFilterPanelOpen,
  onAddFilter,
  onSetFilters,
  onClearFilters,
  namedView,
}: TableFilterToolbarProps) {
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
    // A quick filter may mix AND and OR as long as the expression can be
    // written down as a flat list of conditions with a connector each. Only a
    // parenthesised group of a different operator needs the editor, and that
    // one is refused rather than flattened into a different question.
    const flattened = flattenToConnectors(parsed.value.expression);
    if (!flattened) {
      setQuickFilterError(t('filter.nestedLogic'));
      return;
    }
    const connectors = flattened
      .map((entry) => entry.connector)
      .filter((connector): connector is 'and' | 'or' => connector !== undefined);
    const globalLogic = connectors.length > 0 ? connectors[0] : 'and';
    const conditions: FilterCondition[] = flattened.map((entry) => ({
      column: entry.condition.column,
      operator: entry.condition.operator,
      value: entry.condition.value,
      ...(entry.connector ? { connector: entry.connector } : {}),
    }));
    setQuickFilterError(null);
    onSetFilters(conditions, globalLogic);
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

  const menuProps: NamedViewMenuProps | null = namedView
    ? {
        ...namedView,
        columns: columns.map((column) => column.name),
      }
    : null;

  return (
    <>
      <div className="flex shrink-0 items-start gap-0.5 border-b border-edge px-2 py-0.5">
        {children}
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
        {menuProps && <NamedViewMenu {...menuProps} />}
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
