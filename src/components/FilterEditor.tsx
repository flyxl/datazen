import { ChevronDown, ChevronRight, Plus, X } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import type { FilterCondition, FilterOperator } from '../types';
import { useI18n } from '../hooks/useI18n';
import type { I18nKey } from '../locales';
import { Select } from './ui/Select';
import { Input } from './ui/Input';
import { Button } from './ui/Button';
import { cn } from '../lib/cn';
import { filterDraftEqualsApplied, isCompleteFilter } from '../stores/tableDataStore';
import { formatCell } from '../lib/formatters';

export type FilterLogic = 'and' | 'or';

/** Delay before writing a typed value into the draft (IME-safe). */
export const FILTER_VALUE_DEBOUNCE_MS = 350;

/** Max height for the wrapping filter chip list (~2 rows) before scrolling. */
const FILTER_LIST_MAX_HEIGHT_CLASS = 'max-h-[4.75rem]';

const OPERATORS: FilterOperator[] = [
  'eq',
  'ne',
  'gt',
  'lt',
  'gte',
  'lte',
  'like',
  'in',
  'isNull',
  'isNotNull',
];

interface FilterEditorProps {
  columns: { name: string }[];
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
}

function opLabel(op: FilterOperator, t: (key: I18nKey) => string): string {
  return t(`filter.${op}`);
}

function conditionSummary(f: FilterCondition, t: (key: I18nKey) => string): string {
  const op = opLabel(f.operator, t);
  if (f.operator === 'isNull' || f.operator === 'isNotNull') {
    return `${f.column} ${op}`;
  }
  const value =
    f.value === null || f.value === undefined || f.value === '' ? '…' : formatCell(f.value);
  return `${f.column} ${op} ${value}`;
}

/**
 * Local draft + IME-aware value field.
 * Commits into the filter draft only (no query); parent Apply runs the query.
 */
function FilterValueInput({
  value,
  onCommit,
  placeholder,
  autoFocus,
  onEnterComplete,
}: {
  value: string;
  onCommit: (value: string) => void;
  placeholder: string;
  autoFocus?: boolean;
  /** Called after Enter commits a non-empty value (collapse to chip). */
  onEnterComplete?: () => void;
}) {
  const [draft, setDraft] = useState(value);
  const composingRef = useRef(false);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const draftRef = useRef(draft);
  draftRef.current = draft;

  useEffect(() => {
    if (!composingRef.current) setDraft(value);
  }, [value]);

  useEffect(
    () => () => {
      if (timerRef.current) clearTimeout(timerRef.current);
    },
    [],
  );

  const clearTimer = () => {
    if (timerRef.current) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }
  };

  const flush = (next: string) => {
    clearTimer();
    if (next !== value) onCommit(next);
  };

  const schedule = (next: string) => {
    clearTimer();
    timerRef.current = setTimeout(() => flush(next), FILTER_VALUE_DEBOUNCE_MS);
  };

  return (
    <Input
      autoFocus={autoFocus}
      value={draft}
      onChange={(e) => {
        const next = e.target.value;
        setDraft(next);
        if (composingRef.current || (e.nativeEvent as InputEvent).isComposing) return;
        schedule(next);
      }}
      onCompositionStart={() => {
        composingRef.current = true;
        clearTimer();
      }}
      onCompositionEnd={(e) => {
        const next = e.currentTarget.value;
        setDraft(next);
        requestAnimationFrame(() => {
          composingRef.current = false;
        });
        schedule(next);
      }}
      onBlur={() => {
        if (composingRef.current) return;
        flush(draftRef.current);
      }}
      onKeyDown={(e) => {
        if (e.key === 'Enter' && !composingRef.current && !e.nativeEvent.isComposing) {
          e.preventDefault();
          flush(draftRef.current);
          if (draftRef.current.trim() !== '') onEnterComplete?.();
        }
      }}
      placeholder={placeholder}
      className="!h-7 w-24 shrink-0 !px-2 !text-xs"
      data-testid="filter-value"
    />
  );
}

function FilterConditionChip({
  filter,
  onEdit,
  onRemove,
  t,
}: {
  filter: FilterCondition;
  onEdit: () => void;
  onRemove: () => void;
  t: (key: I18nKey) => string;
}) {
  const label = conditionSummary(filter, t);
  return (
    <div className="inline-flex max-w-full items-center gap-0.5 rounded-full border border-accent/30 bg-accent/10 py-0.5 pl-2 pr-0.5 text-xs text-accent">
      <button
        type="button"
        className="selectable min-w-0 truncate px-0.5 text-left hover:underline"
        onClick={onEdit}
        title={t('filter.editCondition')}
      >
        {label}
      </button>
      <button
        type="button"
        className="shrink-0 rounded-full p-0.5 text-accent/80 hover:bg-accent/20 hover:text-accent"
        onClick={onRemove}
        aria-label={t('filter.remove')}
      >
        <X className="h-3 w-3" />
      </button>
    </div>
  );
}

function FilterConditionEditor({
  filter,
  index,
  columnOptions,
  opOptions,
  onChange,
  onRemove,
  onCollapse,
  autoFocusValue,
  t,
}: {
  filter: FilterCondition;
  index: number;
  columnOptions: { value: string; label: string }[];
  opOptions: { value: string; label: string }[];
  onChange: (index: number, next: FilterCondition) => void;
  onRemove: (index: number) => void;
  onCollapse: () => void;
  autoFocusValue: boolean;
  t: (key: I18nKey) => string;
}) {
  const needsValue = filter.operator !== 'isNull' && filter.operator !== 'isNotNull';
  const rootRef = useRef<HTMLDivElement>(null);
  const latestRef = useRef(filter);
  latestRef.current = filter;

  const pushChange = (next: FilterCondition) => {
    latestRef.current = next;
    onChange(index, next);
  };

  const tryCollapseIfComplete = () => {
    // Defer: Select portal clicks blur the trigger before focus returns / listbox closes.
    globalThis.setTimeout(() => {
      const active = document.activeElement;
      if (rootRef.current?.contains(active)) return;
      if (active?.closest?.('#dz-select-listbox')) return;
      // Listbox still open (picking an option) — stay expanded.
      if (document.getElementById('dz-select-listbox')) return;
      if (isCompleteFilter(latestRef.current)) onCollapse();
    }, 10);
  };

  return (
    <div
      ref={rootRef}
      className="inline-flex max-w-full flex-nowrap items-center gap-1 rounded border border-edge bg-surface-alt/50 py-0.5 pl-0.5 pr-0.5"
      onBlur={(e) => {
        const related = e.relatedTarget as Node | null;
        if (related && rootRef.current?.contains(related)) return;

        // Focus moved to a known outside node — collapse if complete (skip activeElement
        // check; jsdom often leaves activeElement on the blurred input).
        if (related && !rootRef.current?.contains(related)) {
          globalThis.setTimeout(() => {
            if (document.getElementById('dz-select-listbox')) return;
            if (isCompleteFilter(latestRef.current)) onCollapse();
          }, 10);
          return;
        }

        tryCollapseIfComplete();
      }}
    >
      <Select
        value={filter.column}
        options={columnOptions}
        onChange={(column) => pushChange({ ...latestRef.current, column })}
        className="!h-7 w-[6.5rem] shrink-0 !px-1.5 !text-xs"
      />
      <Select
        value={filter.operator}
        options={opOptions}
        onChange={(operator) => {
          const nextOp = operator as FilterOperator;
          const next = { ...latestRef.current, operator: nextOp };
          pushChange(next);
          if (nextOp === 'isNull' || nextOp === 'isNotNull') {
            onCollapse();
          }
        }}
        className="!h-7 w-[4.75rem] shrink-0 !px-1.5 !text-xs"
      />
      {needsValue && (
        <FilterValueInput
          value={filter.value == null ? '' : String(filter.value)}
          onCommit={(value) => pushChange({ ...latestRef.current, value })}
          placeholder={t('filter.value')}
          autoFocus={autoFocusValue}
          onEnterComplete={onCollapse}
        />
      )}
      <button
        type="button"
        className="shrink-0 rounded p-1 text-fg-muted hover:bg-surface-raised hover:text-fg"
        onClick={() => onRemove(index)}
        aria-label={t('filter.remove')}
      >
        <X className="h-3.5 w-3.5" />
      </button>
    </div>
  );
}

export function FilterEditor({
  columns,
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
}: FilterEditorProps) {
  const { t } = useI18n();
  const columnOptions = columns.map((c) => ({ value: c.name, label: c.name }));
  const opOptions = OPERATORS.map((op) => ({ value: op, label: t(`filter.${op}`) }));

  const dirty = !filterDraftEqualsApplied(draftFilters, draftLogic, appliedFilters, appliedLogic);

  const appliedCount = appliedFilters.length;
  const summaryText =
    appliedCount === 0
      ? t('filter.noActive')
      : appliedFilters.map((f) => conditionSummary(f, t)).join(` ${appliedLogic.toUpperCase()} `);

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
    <div className="shrink-0 border-b border-edge bg-surface" data-testid="filter-editor">
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
            disabled={columns.length === 0}
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
              >
                {t('filter.or')}
              </button>
            </div>
            <Button
              variant="ghost"
              className="h-6 shrink-0 gap-1 px-1.5 text-xs"
              onClick={addEmpty}
              disabled={columns.length === 0}
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
                data-testid="filter-collapse"
              >
                {t('filter.collapse')}
              </Button>
              <Button
                className="h-6 px-2.5 text-xs"
                disabled={!dirty}
                data-testid="filter-apply"
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
            {draftFilters.length === 0 ? (
              <div className="px-1 py-1 text-xs text-fg-muted">{t('filter.noActive')}</div>
            ) : (
              draftFilters.map((f, idx) => {
                const complete = isCompleteFilter(f);
                const expanded = !complete || editingIndex === idx;
                const isLast = idx === draftFilters.length - 1;

                return (
                  <div
                    key={`${f.column}-${f.operator}-${idx}`}
                    ref={isLast ? lastItemRef : undefined}
                    className="max-w-full"
                  >
                    {expanded ? (
                      <FilterConditionEditor
                        filter={f}
                        index={idx}
                        columnOptions={columnOptions}
                        opOptions={opOptions}
                        onChange={onChange}
                        onRemove={onRemove}
                        onCollapse={() => setEditingIndex(null)}
                        autoFocusValue={!complete || editingIndex === idx}
                        t={t}
                      />
                    ) : (
                      <FilterConditionChip
                        filter={f}
                        onEdit={() => setEditingIndex(idx)}
                        onRemove={() => onRemove(idx)}
                        t={t}
                      />
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
