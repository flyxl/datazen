import { ChevronDown, ChevronRight, Download, Plus, Terminal } from 'lucide-react';
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
import { defaultOperatorForColumn } from '../lib/filterOperators';
import type { CellTypeHints } from '../lib/cellTypes';
import type { FilterColumn } from './filter/types';

export type FilterLogic = 'and' | 'or';

/**
 * The editor panes of the filter panel (DBR-04). The structured editor is
 * the whole panel every build before the mode row shipped; the two new panes
 * are offered only when the caller passes their props, so a caller that does
 * not renders the panel it always had.
 */
export type FilterPanelMode = 'structured' | 'anyColumn' | 'rawSql';

/**
 * The debounce of the value field lives with the value field now; the panel
 * re-exports it because the existing tests import it from here.
 */
export { FILTER_VALUE_DEBOUNCE_MS } from './filter/FilterValueInput';

/** Max height for the wrapping filter chip list (~2 rows) before scrolling. */
const FILTER_LIST_MAX_HEIGHT_CLASS = 'max-h-[4.75rem]';

export interface FilterEditorProps {
  columns: FilterColumn[];
  /**
   * The filter operators this panel offers.
   *
   * The list is **already resolved**: whoever read the driver's declaration
   * turned it into a list with `operatorsForDriver` — an empty declaration
   * means "said nothing", not "serves nothing", and that rule lives in the
   * library so the shipped path and the tested path cannot drift apart.
   *
   * Omit the prop and the panel offers the shared set.
   */
  supportedOperators?: readonly FilterOperator[];
  /**
   * The driver's own column-type rules (`DatabaseTypeMeta.cellTypes`) — the
   * same declaration the cell editor reads, so a column the driver calls a
   * boolean is a boolean in the filter too (08 §3.5).
   */
  cellTypeHints?: CellTypeHints;
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
  /**
   * Show the statements this panel's read would send (DB-15). Absent — a
   * panel that cannot reach the backend's preview — no button is rendered.
   */
  onPreviewSql?: () => void;
  /**
   * The direct exit to the export dialog with "current filter results"
   * preselected (DB-15). Absent — a panel whose filtered export cannot be
   * built — no button is rendered.
   */
  onExportFiltered?: () => void;
  /* ── The DBR-04 modes. Every prop is optional; a caller that passes no
      `mode` renders the panel exactly as it was before the mode row. ── */
  /** The pane the panel shows; absent means the structured editor only. */
  mode?: FilterPanelMode;
  /** Switch panes. Render-only: switching never executes anything. */
  onModeChange?: (mode: FilterPanelMode) => void;
  /** Whether the Raw SQL pane is offered at all (the driver's declaration). */
  rawWhereSupported?: boolean;
  /** The Any Column draft: one value, searched across the searchable columns. */
  anyColumnValue?: string;
  onAnyColumnValueChange?: (value: string) => void;
  /** Apply the Any Column draft. Absent — the pane is not offered. */
  onApplyAnyColumn?: (value: string) => void;
  /** How many columns the Any Column search would skip (not text-searchable). */
  anyColumnSkippedCount?: number;
  /** How many columns the Any Column search would actually search. */
  anyColumnSearchableCount?: number;
  /** The Raw SQL draft: one boolean `WHERE` predicate. */
  rawWhereValue?: string;
  onRawWhereValueChange?: (value: string) => void;
  /** Apply the Raw SQL draft. Absent — the pane is not offered. */
  onApplyRawWhere?: (value: string) => void;
  /** The applied raw predicate, for the collapsed summary and the badge. */
  appliedRawWhere?: string | null;
  /**
   * Where the raw predicate will run (DBR-04): `database · schema`, as the
   * panel sees it. Absent — no context line is rendered.
   */
  rawWhereContext?: string | null;
}

export function FilterEditor({
  columns,
  supportedOperators,
  cellTypeHints,
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
  onPreviewSql,
  onExportFiltered,
  mode = 'structured',
  onModeChange,
  rawWhereSupported = false,
  anyColumnValue = '',
  onAnyColumnValueChange,
  onApplyAnyColumn,
  anyColumnSkippedCount,
  anyColumnSearchableCount,
  rawWhereValue = '',
  onRawWhereValueChange,
  onApplyRawWhere,
  appliedRawWhere = null,
  rawWhereContext = null,
}: FilterEditorProps) {
  const { t } = useI18n();

  /**
   * The operators this panel offers.
   *
   * A caller that passes a list has already resolved it (see the prop); a
   * caller that passes nothing gets the shared set, which is the same list
   * `operatorsForDriver` falls back to.
   */
  const operators = supportedOperators ?? SHARED_FILTER_OPERATORS;

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
  /** The applied raw predicate, when the Raw SQL channel is the applied one. */
  const rawApplied = appliedRawWhere != null && appliedRawWhere !== '';
  /** One applied channel at a time (DBR-04): the badge counts either, never both. */
  const badgeCount = rawApplied ? 1 : appliedCount;
  /** Whether the mode row is rendered at all: only with someone to switch to. */
  const modeRowVisible = onModeChange != null && (onApplyAnyColumn != null || rawWhereSupported);
  /**
   * The summary of the applied filters.
   *
   * Every condition is joined with the connector that is actually in effect
   * for it — its own one, or the request wide logic. Joining everything with
   * the request wide logic would misread a mixed list.
   */
  const summaryText =
    rawApplied && appliedCount === 0
      ? t('filter.rawAppliedSummary', { predicate: appliedRawWhere ?? '' })
      : appliedCount === 0
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

  if (!open && appliedCount === 0 && draftFilters.length === 0 && !rawApplied) {
    return null;
  }

  const addEmpty = () => {
    const column = columns[0];
    onAdd({
      column: column?.name ?? '',
      // The default operator belongs to the column type: `eq` today, but a
      // type that cannot take `eq` needs its own first operator.
      operator: defaultOperatorForColumn(column?.type, cellTypeHints),
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
          {badgeCount > 0 && (
            <span className="shrink-0 rounded bg-accent/15 px-1.5 py-px text-[10px] font-medium text-accent">
              {rawApplied
                ? t('filter.rawBadge')
                : t('filter.activeCount').replace('{count}', String(appliedCount))}
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
        {badgeCount > 0 && (
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
          {modeRowVisible && (
            <div
              className="mb-1.5 inline-flex shrink-0 rounded border border-edge"
              role="tablist"
              aria-label={t('filter.modeRowLabel')}
            >
              {(
                [
                  ['structured', 'filter.modeStructured'],
                  ...(onApplyAnyColumn != null
                    ? ([['anyColumn', 'filter.modeAnyColumn']] as const)
                    : []),
                  ...(rawWhereSupported ? ([['rawSql', 'filter.modeRawSql']] as const) : []),
                ] as const
              ).map(([value, labelKey]) => (
                <button
                  key={value}
                  type="button"
                  role="tab"
                  aria-selected={mode === value}
                  className={cn(
                    'px-1.5 py-0.5 text-[11px]',
                    mode === value ? 'bg-accent/15 text-accent' : 'text-fg-secondary',
                  )}
                  onClick={() => onModeChange?.(value)}
                  disabled={loading}
                  data-testid={`filter-mode-${value}`}
                >
                  {t(labelKey)}
                </button>
              ))}
            </div>
          )}
          {mode === 'anyColumn' && onApplyAnyColumn != null ? (
            <div className="flex flex-col gap-1.5" data-testid="filter-any-column-pane">
              {applyError && (
                <ErrorBanner className="basis-full px-1 py-1" data-testid="filter-error">
                  {applyError}
                </ErrorBanner>
              )}
              <div className="flex items-center gap-1.5">
                <input
                  type="text"
                  value={anyColumnValue}
                  onChange={(event) => onAnyColumnValueChange?.(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === 'Enter' && !event.nativeEvent.isComposing) {
                      event.preventDefault();
                      if (anyColumnValue.trim() && !loading) onApplyAnyColumn(anyColumnValue);
                    }
                  }}
                  disabled={loading}
                  placeholder={t('filter.anyColumnPlaceholder')}
                  className="h-7 min-w-0 flex-1 rounded border border-edge bg-surface px-2 text-xs text-fg placeholder:text-fg-muted focus:border-accent focus:outline-none"
                  data-testid="any-column-input"
                />
                <Button
                  className="h-6 shrink-0 px-2.5 text-xs"
                  disabled={!anyColumnValue.trim() || loading}
                  onClick={() => onApplyAnyColumn(anyColumnValue)}
                  data-testid="any-column-apply"
                >
                  {t('filter.apply')}
                </Button>
              </div>
              <div className="text-[10px] text-fg-muted">
                {t('filter.anyColumnScope', {
                  searchable: String(anyColumnSearchableCount ?? 0),
                  skipped: String(anyColumnSkippedCount ?? 0),
                })}
              </div>
            </div>
          ) : mode === 'rawSql' && rawWhereSupported && onApplyRawWhere != null ? (
            <div className="flex flex-col gap-1.5" data-testid="filter-raw-sql-pane">
              {applyError && (
                <ErrorBanner className="basis-full px-1 py-1" data-testid="filter-error">
                  {applyError}
                </ErrorBanner>
              )}
              <div className="flex items-center gap-1.5">
                <input
                  type="text"
                  value={rawWhereValue}
                  onChange={(event) => onRawWhereValueChange?.(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === 'Enter' && !event.nativeEvent.isComposing) {
                      event.preventDefault();
                      if (rawWhereValue.trim() && !loading) onApplyRawWhere(rawWhereValue);
                    }
                  }}
                  disabled={loading}
                  placeholder={t('filter.rawPlaceholder')}
                  spellCheck={false}
                  className="h-7 min-w-0 flex-1 rounded border border-edge bg-surface px-2 font-mono text-xs text-fg placeholder:text-fg-muted focus:border-accent focus:outline-none"
                  data-testid="raw-where-input"
                />
                <Button
                  className="h-6 shrink-0 px-2.5 text-xs"
                  disabled={!rawWhereValue.trim() || loading}
                  onClick={() => onApplyRawWhere(rawWhereValue)}
                  data-testid="raw-where-apply"
                >
                  {t('filter.apply')}
                </Button>
              </div>
              <div className="text-[10px] text-fg-muted" data-testid="raw-where-hint">
                {rawWhereContext
                  ? t('filter.rawQueryContext', { context: rawWhereContext })
                  : t('filter.rawHint')}
              </div>
            </div>
          ) : (
            <>
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
                  {onExportFiltered && (
                    <Button
                      variant="ghost"
                      className="h-6 shrink-0 gap-1 px-2 text-xs"
                      onClick={onExportFiltered}
                      disabled={loading}
                      data-testid="filter-export-filtered"
                      title={t('filter.exportFilteredHint')}
                    >
                      <Download className="h-3 w-3" />
                      {t('filter.exportFiltered')}
                    </Button>
                  )}
                  {onPreviewSql && (
                    <Button
                      variant="ghost"
                      className="h-6 shrink-0 gap-1 px-2 font-mono text-xs"
                      onClick={onPreviewSql}
                      disabled={loading}
                      data-testid="filter-preview-sql"
                      title={t('filter.previewSqlHint')}
                    >
                      <Terminal className="h-3 w-3" />
                      SQL
                    </Button>
                  )}
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
                      incomplete
                        ? t('filter.errorIncomplete', { column: incomplete.column })
                        : undefined
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
                            supportedOperators={operators}
                            cellTypeHints={cellTypeHints}
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
            </>
          )}
        </div>
      )}
    </div>
  );
}
