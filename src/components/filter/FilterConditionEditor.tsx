import { useMemo, useRef } from 'react';
import { X } from 'lucide-react';
import type { FilterCondition, FilterOperator } from '../../types';
import { Select } from '../ui/Select';
import { isCompleteFilter } from '../../stores/tableData/filterUtils';
import {
  convertFilterValue,
  normalizeOperatorForColumn,
  operatorsForColumn,
  valueInputFor,
} from '../../lib/filterOperators';
import type { I18nKey } from '../../locales';
import { FilterValueInput } from './FilterValueInput';
import { FilterTokenInput } from './FilterTokenInput';
import { FilterBetweenInput } from './FilterBetweenInput';
import type { FilterColumn } from './types';
import { toPair, toTokens } from './filterValues';

/** The open select listbox of the shared `Select` component. */
const SELECT_LISTBOX_SELECTOR = '[id^="dz-select-listbox-"]';

/**
 * One expanded condition: the column, the operator, the value and the remove
 * button.
 *
 * The component owns one piece of state only — which value it currently shows
 * — and reports every change to the parent, which owns the draft.
 *
 * **The operator list depends on the column.** A pattern operator on a
 * numeric column would be a query the database is guaranteed to reject, so the
 * list is the intersection of "the column type takes it" and "this database
 * serves it". The draft's current operator stays visible even when it is not
 * in that list: the row holds what was put there, and hiding it would make
 * the editor lie about the draft.
 */
export function FilterConditionEditor({
  filter,
  index,
  columns,
  supportedOperators,
  likeCaseInsensitive = false,
  onChange,
  onRemove,
  onCollapse,
  autoFocusValue,
  disabled,
  t,
}: {
  filter: FilterCondition;
  index: number;
  columns: readonly FilterColumn[];
  supportedOperators: readonly FilterOperator[];
  /**
   * Whether this database ignores case in `LIKE`. Copy only: the hint says
   * what the database does, it never changes the query.
   */
  likeCaseInsensitive?: boolean;
  onChange: (index: number, next: FilterCondition) => void;
  onRemove: (index: number) => void;
  onCollapse: () => void;
  autoFocusValue: boolean;
  disabled: boolean;
  t: (key: I18nKey) => string;
}) {
  const rootRef = useRef<HTMLDivElement>(null);
  const latestRef = useRef(filter);
  latestRef.current = filter;

  const columnType = useMemo(
    () => columns.find((column) => column.name === filter.column)?.type,
    [columns, filter.column],
  );

  const columnOptions = useMemo(
    () => columns.map((column) => ({ value: column.name, label: column.name })),
    [columns],
  );

  const opOptions = useMemo(() => {
    const operators = operatorsForColumn(columnType, supportedOperators);
    const options = operators.map((operator) => ({
      value: operator,
      label: t(`filter.${operator}`),
    }));
    if (!operators.includes(filter.operator)) {
      options.push({ value: filter.operator, label: t(`filter.${filter.operator}`) });
    }
    return options;
  }, [columnType, supportedOperators, filter.operator, t]);

  const valueKind = valueInputFor(filter.operator);

  const pushChange = (next: FilterCondition) => {
    latestRef.current = next;
    onChange(index, next);
  };

  const tryCollapseIfComplete = () => {
    // Defer: Select portal clicks blur the trigger before focus returns / listbox closes.
    globalThis.setTimeout(() => {
      const active = document.activeElement;
      if (rootRef.current?.contains(active)) return;
      if (active?.closest?.(SELECT_LISTBOX_SELECTOR)) return;
      // Listbox still open (picking an option) — stay expanded.
      if (document.querySelector(SELECT_LISTBOX_SELECTOR)) return;
      if (isCompleteFilter(latestRef.current)) onCollapse();
    }, 10);
  };

  const changeOperator = (operator: FilterOperator) => {
    const from = valueInputFor(latestRef.current.operator);
    const to = valueInputFor(operator);
    const next: FilterCondition = {
      ...latestRef.current,
      operator,
      value: convertFilterValue(from, to, latestRef.current.value),
    };
    pushChange(next);
    // An operator without a value has nothing left to edit: collapse to the chip.
    if (to === 'none') onCollapse();
  };

  const changeColumn = (column: string) => {
    const nextType = columns.find((entry) => entry.name === column)?.type;
    const operator = normalizeOperatorForColumn(
      latestRef.current.operator,
      nextType,
      supportedOperators,
    );
    const next: FilterCondition = {
      ...latestRef.current,
      column,
      operator,
      value: convertFilterValue(
        valueInputFor(latestRef.current.operator),
        valueInputFor(operator),
        latestRef.current.value,
      ),
    };
    pushChange(next);
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
            if (document.querySelector(SELECT_LISTBOX_SELECTOR)) return;
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
        disabled={disabled}
        onChange={(column) => changeColumn(column)}
        className="!h-7 w-[6.5rem] shrink-0 !px-1.5 !text-xs"
      />
      <Select
        value={filter.operator}
        options={opOptions}
        disabled={disabled}
        onChange={(operator) => changeOperator(operator as FilterOperator)}
        className="!h-7 w-[4.75rem] shrink-0 !px-1.5 !text-xs"
      />
      {valueKind === 'single' && (
        <>
          <FilterValueInput
            value={filter.value == null ? '' : String(filter.value)}
            onCommit={(value) => pushChange({ ...latestRef.current, value })}
            placeholder={t('filter.value')}
            autoFocus={autoFocusValue}
            disabled={disabled}
            onEnterComplete={onCollapse}
          />
          {likeCaseInsensitive && filter.operator === 'like' && (
            <span
              className="max-w-[12rem] truncate text-[11px] text-fg-muted"
              title={t('filter.likeCaseInsensitiveHint')}
              data-testid="filter-like-case-insensitive"
            >
              {t('filter.likeCaseInsensitiveHint')}
            </span>
          )}
        </>
      )}
      {valueKind === 'tokens' && (
        <FilterTokenInput
          value={toTokens(filter.value)}
          onCommit={(tokens) => pushChange({ ...latestRef.current, value: tokens })}
          placeholder={t('filter.tokensPlaceholder')}
          autoFocus={autoFocusValue}
          disabled={disabled}
          onEnterComplete={onCollapse}
          removeLabel={t('filter.tokenRemove')}
        />
      )}
      {valueKind === 'pair' && (
        <FilterBetweenInput
          value={toPair(filter.value)}
          onCommit={(bounds) => pushChange({ ...latestRef.current, value: bounds })}
          placeholder={t('filter.value')}
          connectorLabel={t('filter.and')}
          invertedLabel={t('filter.betweenInverted')}
          autoFocus={autoFocusValue}
          disabled={disabled}
        />
      )}
      <button
        type="button"
        className="shrink-0 rounded p-1 text-fg-muted hover:bg-surface-raised hover:text-fg"
        disabled={disabled}
        onClick={() => onRemove(index)}
        aria-label={t('filter.remove')}
      >
        <X className="h-3.5 w-3.5" />
      </button>
    </div>
  );
}
