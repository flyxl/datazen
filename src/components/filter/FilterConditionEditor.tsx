import { useRef } from 'react';
import { X } from 'lucide-react';
import type { FilterCondition, FilterOperator } from '../../types';
import { Select } from '../ui/Select';
import { isCompleteFilter } from '../../stores/tableData/filterUtils';
import type { I18nKey } from '../../locales';
import { FilterValueInput } from './FilterValueInput';

/** The open select listbox of the shared `Select` component. */
const SELECT_LISTBOX_SELECTOR = '[id^="dz-select-listbox-"]';

/**
 * One expanded condition: the column, the operator, the value and the remove
 * button.
 *
 * The component owns one piece of state only — which value it currently shows
 * — and reports every change to the parent, which owns the draft.
 */
export function FilterConditionEditor({
  filter,
  index,
  columnOptions,
  opOptions,
  onChange,
  onRemove,
  onCollapse,
  autoFocusValue,
  disabled,
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
  disabled: boolean;
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
      if (active?.closest?.(SELECT_LISTBOX_SELECTOR)) return;
      // Listbox still open (picking an option) — stay expanded.
      if (document.querySelector(SELECT_LISTBOX_SELECTOR)) return;
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
        onChange={(column) => pushChange({ ...latestRef.current, column })}
        className="!h-7 w-[6.5rem] shrink-0 !px-1.5 !text-xs"
      />
      <Select
        value={filter.operator}
        options={opOptions}
        disabled={disabled}
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
          disabled={disabled}
          onEnterComplete={onCollapse}
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
