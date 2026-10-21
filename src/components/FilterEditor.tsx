import { ChevronDown, ChevronRight, Plus } from 'lucide-react';
import { useEffect, useMemo, useRef, useState } from 'react';
import type { FilterCondition, FilterOperator } from '../types';
import { SHARED_FILTER_OPERATORS } from '../types';
import { useI18n } from '../hooks/useI18n';
import { Button } from './ui/Button';
import { ErrorBanner } from './ui/ErrorBanner';
import { cn } from '../lib/cn';
import { filterDraftEqualsApplied, isCompleteFilter } from '../stores/tableData/filterUtils';
import { FilterConditionChip } from './filter/FilterConditionChip';
import { FilterConditionEditor } from './filter/FilterConditionEditor';
import { FilterConnectorToggle } from './filter/FilterConnectorToggle';
import { conditionSummary } from './filter/filterLabels';
import type { FilterColumn } from './filter/types';

export type FilterLogic = 'and' | 'or';

/**
 * The debounce of the value field lives with the value field now; the panel
 * re-exports it because the existing tests import it from here.
 */
export { FILTER_VALUE_DEBOUNCE_MS } from './filter/FilterValueInput';

/** Max height for the wrapping filter chip list (~2 rows) before scrolling. */
const FILTER_LIST_MAX_HEIGHT_CLASS = 'max-h-[4.75rem]';

export interface FilterEditorProps {
  columns: FilterColumn[];
  /** The filter operators this database serves; defaults to the shared set. */
  supportedOperators?: readonly FilterOperator[];
  /**
   * Whether this database ignores case in `LIKE` under its default collation.
   * Copy only — it never changes the SQL.
   */
  likeCaseInsensitive?: boolean;
  /** Currently applied filters (shown in collapsed summary). */
  appliedFilters: FilterCondition[];
  appliedLogic: FilterLogic;
  draftFilters: FilterCondition[];
  draftLogic: FilterLogic;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onLogicChange: (logic: FilterLogic) => void;
  onChange: (index: number, next: FilterCondition) => void;
  onAdd: (filter: FilterCondition) => void;
  onRemove: (index: number) => void;
  onApply: () => void;
  onClear: () => void;
  /** Prevents mutations while the applied filter request is in flight. */
  loading?: boolean;
  /** Caller-owned parse/apply error; this editor does not execute requests. */
  applyError?: string | null;
}

export function FilterEditor({
  columns,
  supportedOperators = SHARED_FILTER_OPERATORS,
  likeCaseInsensitive = false,
  appliedFilters,
  appliedLogic,
  draftFilters,
  draftLogic,
  open,
  onOpenChange,
  onLogicChange,
  onChange,
  onAdd,
  onRemove,
  onApply,
  onClear,
  loading = false,
  applyError = null,
}: FilterEditorProps) {
  const { t } = useI18n();

  const dirty = !filterDraftEqualsApplied(draftFilters, draftLogic, appliedFilters, appliedLogic);

  /**
   * The first condition that cannot be sent to the database.
   *
   * An incomplete condition is never dropped silently: it is marked in the
   * list and it blocks `Apply`, so the user either completes it or removes
   * it. The back end refuses such a condition as well (`grid.filter.incomplete`).
   */
  const incomplete = useMemo(
    () => draftFilters.find((filter) => !isCompleteFilter(filter)),
    [draftFilters],
  );

  const appliedCount = appliedFilters.length;
  /**
   * The summary of the applied filters.
   *
   * Every condition is joined with the connector that is actually in effect
   * for it — its own one, or the request wide logic. Joining everything with
   * the request wide logic would misread a mixed list.
   */
  const summaryText =
    appliedCount === 0
      ? t('filter.noActive')
      : appliedFilters
          .map((filter, index) => {
            const condition = conditionSummary(filter, t);
            if (index === 0) return condition;
            return `${(filter.connector ?? appliedLogic).toUpperCase()} ${condition}`;
          })
          .join(' ');

  /**
   * Whether a condition carries a connector of its own.
   *
   * The first condition has no predecessor, so its own connector is
   * meaningless; every other condition either names its connector or follows
   * the request wide logic.
   */
  const perConditionLogic = useMemo(
    () => draftFilters.slice(1).some((filter) => filter.connector !== undefined),
    [draftFilters],
  );

  /**
   * Give one condition its own connector.
   *
   * The spread keeps the key order of the condition — `JSON.stringify` is
   * order sensitive and the draft comparison would call a rewritten condition
   * "changed" (or "unchanged") for the wrong reason.
   */
  const setConnector = (index: number, connector: 'and' | 'or') => {
    const filter = draftFilters[index];
    if (!filter) return;
    onChange(index, { ...filter, connector });
  };

  /** Drop every own connector, so the request wide logic is the only truth again. */
  const resetConnectors = () => {
    draftFilters.forEach((filter, index) => {
      if (filter.connector === undefined) return;
      const rest = { ...filter };
      delete rest.connector;
      onChange(index, rest);
    });
  };

  /** Index of the filter currently shown as full inputs; null = all complete → chips. */
  const [editingIndex, setEditingIndex] = useState<number | null>(null);
  const lastItemRef = useRef<HTMLDivElement | null>(null);
  const prevCountRef = useRef(draftFilters.length);

  useEffect(() => {
    if (draftFilters.length > prevCountRef.current) {
      setEditingIndex(draftFilters.length - 1);
      requestAnimationFrame(() => {
        lastItemRef.current?.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
      });
    } else if (editingIndex != null && editingIndex >= draftFilters.length) {
      setEditingIndex(draftFilters.length === 0 ? null : draftFilters.length - 1);
    }
    prevCountRef.current = draftFilters.length;
  }, [draftFilters.length, editingIndex]);

  if (!open && appliedCount === 0 && draftFilters.length === 0) {
    return null;
  }

  const addEmpty = () => {
    onAdd({
      column: columns[0]?.name ?? '',
      operator: 'eq',
      value: '',
    });
  };

  return (
    <div
      className="shrink-0 border-b border-edge bg-surface"
      data-testid="filter-editor"
      aria-busy={loading}
      aria-disabled={loading || undefined}
    >
      <div className="flex h-8 items-center gap-1.5 px-2">
        <button
          type="button"
          className="flex min-w-0 flex-1 items-center gap-1.5 rounded px-1 py-0.5 text-left hover:bg-surface-raised"
          onClick={() => onOpenChange(!open)}
          aria-expanded={open}
          data-testid="filter-summary-toggle"
        >
          {open ? (
            <ChevronDown className="h-3.5 w-3.5 shrink-0 text-fg-muted" />
          ) : (
            <ChevronRight className="h-3.5 w-3.5 shrink-0 text-fg-muted" />
          )}
          <span className="shrink-0 text-[11px] font-semibold uppercase tracking-wider text-fg-muted">
            {t('filter.filter')}
          </span>
          {appliedCount > 0 && (
            <span className="shrink-0 rounded bg-accent/15 px-1.5 py-px text-[10px] font-medium text-accent">
              {t('filter.activeCount').replace('{count}', String(appliedCount))}
            </span>
          )}
          {dirty && (
            <span className="shrink-0 text-[10px] text-amber-400">{t('filter.unapplied')}</span>
          )}
          <span className="min-w-0 flex-1 truncate text-xs text-fg-secondary" title={summaryText}>
            {summaryText}
          </span>
        </button>
        {!open && (
          <Button
            variant="ghost"
            className="h-6 shrink-0 gap-1 px-1.5 text-xs"
            onClick={() => {
              onOpenChange(true);
              addEmpty();
            }}
            disabled={columns.length === 0 || loading}
          >
            <Plus className="h-3 w-3" />
            {t('filter.add')}
          </Button>
        )}
        {appliedCount > 0 && (
          <button
            type="button"
            className="shrink-0 px-1.5 text-xs text-fg-secondary hover:text-fg"
            onClick={onClear}
            disabled={loading}
            data-testid="filter-clear"
          >
            {t('filter.clear')}
          </button>
        )}
      </div>

      {open && (
        <div className="border-t border-edge px-2 pb-2 pt-1.5">
          <div className="mb-1.5 flex flex-nowrap items-center gap-1.5">
            <div className="inline-flex shrink-0 rounded border border-edge">
              <button
                type="button"
                className={cn(
                  'px-1.5 py-0.5 text-[11px]',
                  draftLogic === 'and' ? 'bg-accent/15 text-accent' : 'text-fg-secondary',
                )}
                onClick={() => onLogicChange('and')}
                disabled={loading || perConditionLogic}
                title={perConditionLogic ? t('filter.globalLogicDisabledHint') : undefined}
                data-testid="filter-logic-and"
              >
                {t('filter.and')}
              </button>
              <button
                type="button"
                className={cn(
                  'px-1.5 py-0.5 text-[11px]',
                  draftLogic === 'or' ? 'bg-accent/15 text-accent' : 'text-fg-secondary',
                )}
                onClick={() => onLogicChange('or')}
                disabled={loading || perConditionLogic}
                title={perConditionLogic ? t('filter.globalLogicDisabledHint') : undefined}
                data-testid="filter-logic-or"
              >
                {t('filter.or')}
              </button>
            </div>
            {perConditionLogic && (
              <button
                type="button"
                className="shrink-0 rounded px-1.5 py-0.5 text-[11px] text-fg-secondary hover:text-fg"
                onClick={resetConnectors}
                disabled={loading}
                data-testid="filter-connector-reset"
              >
                {t('filter.connectorReset', { logic: draftLogic.toUpperCase() })}
              </button>
            )}
            <Button
              variant="ghost"
              className="h-6 shrink-0 gap-1 px-1.5 text-xs"
              onClick={addEmpty}
              disabled={columns.length === 0 || loading}
              data-testid="filter-add"
            >
              <Plus className="h-3 w-3" />
              {t('filter.add')}
            </Button>
            <div className="ml-auto flex shrink-0 items-center gap-1.5">
              <Button
                variant="ghost"
                className="h-6 px-2 text-xs"
                onClick={() => onOpenChange(false)}
                disabled={loading}
                data-testid="filter-collapse"
              >
                {t('filter.collapse')}
              </Button>
              <Button
                className="h-6 px-2.5 text-xs"
                disabled={!dirty || loading || incomplete !== undefined}
                data-testid="filter-apply"
                title={
                  incomplete ? t('filter.errorIncomplete', { column: incomplete.column }) : undefined
                }
                onClick={() => {
                  setEditingIndex(null);
                  onApply();
                }}
              >
                {t('filter.apply')}
              </Button>
            </div>
          </div>

          <div
            className={cn(
              'flex flex-wrap content-start gap-1.5 overflow-y-auto overflow-x-hidden',
              FILTER_LIST_MAX_HEIGHT_CLASS,
            )}
          >
            {applyError && (
              <ErrorBanner className="basis-full px-1 py-1" data-testid="filter-error">
                {applyError}
              </ErrorBanner>
            )}
            {incomplete && (
              <ErrorBanner
                className="basis-full px-1 py-1"
                data-testid="filter-incomplete-error"
              >
                {t('filter.errorIncomplete', { column: incomplete.column })}
              </ErrorBanner>
            )}
            {draftFilters.length === 0 ? (
              <div className="px-1 py-1 text-xs text-fg-muted">{t('filter.noActive')}</div>
            ) : (
              draftFilters.map((f, idx) => {
                const complete = isCompleteFilter(f);
                const expanded = !complete || editingIndex === idx;
                const isLast = idx === draftFilters.length - 1;
                // The first condition has no predecessor: its own connector
                // would be meaningless, so no badge is rendered for it.
                const connector = idx > 0 ? (f.connector ?? draftLogic) : null;
                const explicit = f.connector !== undefined;

                return (
                  <div
                    key={`${f.column}-${f.operator}-${idx}`}
                    ref={isLast ? lastItemRef : undefined}
                    className="flex max-w-full flex-nowrap items-center gap-1"
                    data-filter-index={idx}
                  >
                    {connector && (
                      <FilterConnectorToggle
                        index={idx}
                        value={connector}
                        explicit={explicit}
                        differsFromGlobal={explicit && connector !== draftLogic}
                        disabled={loading}
                        onSelect={(next) => setConnector(idx, next)}
                        t={t}
                      />
                    )}
                    {expanded ? (
                      <FilterConditionEditor
                        filter={f}
                        index={idx}
                        columns={columns}
                        supportedOperators={supportedOperators}
                        likeCaseInsensitive={likeCaseInsensitive}
                        onChange={onChange}
                        onRemove={onRemove}
                        onCollapse={() => setEditingIndex(null)}
                        autoFocusValue={!complete || editingIndex === idx}
                        disabled={loading}
                        t={t}
                      />
                    ) : (
                      <FilterConditionChip
                        filter={f}
                        onEdit={() => setEditingIndex(idx)}
                        onRemove={() => onRemove(idx)}
                        disabled={loading}
                        t={t}
                      />
                    )}
                    {!complete && (
                      <span
                        className="ml-1 rounded bg-warning/15 px-1 py-px text-[10px] text-warning"
                        data-testid="filter-incomplete"
                      >
                        {t('filter.incomplete')}
                      </span>
                    )}
                  </div>
                );
              })
            )}
          </div>
        </div>
      )}
    </div>
  );
}
