import { X } from 'lucide-react';
import { useI18n } from '../../hooks/useI18n';
import type { FilterCondition, FilterOperator } from '../../types';
import type { DataSyncRecordset } from '../../commands/sync';
import { Button } from '../../components/ui/Button';
import { Input } from '../../components/ui/Input';
import { Select } from '../../components/ui/Select';

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

export interface StructuredSourceFilter {
  filters: FilterCondition[];
  logic?: 'and' | 'or';
  /** Sync-only row range; preserved while editing the predicate. */
  recordset?: DataSyncRecordset;
}

interface SourceFilterEditorProps {
  columns: string[];
  filter?: StructuredSourceFilter;
  onChange: (filter: StructuredSourceFilter | undefined) => void;
  /** §8.4: held inert while the transfer job prepares. */
  disabled?: boolean;
}

export function SourceFilterEditor({
  columns,
  filter,
  onChange,
  disabled = false,
}: SourceFilterEditorProps) {
  const { t } = useI18n();
  const conditions = filter?.filters ?? [];
  const columnOptions = columns.map((column) => ({ value: column, label: column }));
  const operatorOptions = OPERATORS.map((operator) => ({
    value: operator,
    label: t(`filter.${operator}`),
  }));

  /**
   * DEFENCE IN DEPTH — THIS GUARD IS NOT A FIX, and removing it changes no
   * test result. Recorded by mutation, not asserted: deleting the
   * `if (disabled) return;` line below leaves the whole data-transfer suite
   * green (62/62, `VITEST_EXIT=0`, mutant N3, worktree sha
   * 68aefaa2ec6cf23aafd3d8a19cb1547f7b31b110).
   *
   * The guard that actually holds this editor inert while the prepare is in
   * flight is `ColumnMappingEditor.commit` — pre-existing, and itself proven
   * load-bearing: deleting its guard kills the pre-existing test
   * `holds the mapping editor inert while the prepare is in flight (§8.4)`
   * (mutant N4). Every `onChange` site in this component routes through
   * `commit` at the parent, so `commit` is the effective choke point and this
   * one is a second, redundant gate behind it.
   *
   * It is kept because it is the only way out of this component and it costs
   * nothing, and because it keeps the invariant legible at the component that
   * owns it rather than only one layer up. But do not read this line as
   * evidence that a defect was fixed here, and do not report it as one.
   */
  const apply = (next: StructuredSourceFilter | undefined) => {
    if (disabled) return;
    onChange(next);
  };

  const update = (next: FilterCondition[]) => {
    apply(
      next.length > 0
        ? {
            filters: next,
            logic: filter?.logic ?? 'and',
            ...(filter?.recordset ? { recordset: filter.recordset } : {}),
          }
        : filter?.recordset
          ? { filters: [], recordset: filter.recordset }
          : undefined,
    );
  };

  const add = () => {
    const column = columns[0] ?? '';
    if (!column) return;
    update([...conditions, { column, operator: 'eq', value: '' }]);
  };

  return (
    <div
      className="space-y-2 rounded-lg border border-edge bg-surface-alt p-3"
      data-testid="data-transfer-source-filter"
    >
      <div className="flex items-center justify-between gap-2">
        <div>
          <div className="text-sm font-medium">{t('transfer.mapping.sourceFilter')}</div>
          <div className="text-xs text-fg-muted">{t('transfer.mapping.sourceFilterHint')}</div>
        </div>
        <Button
          variant="ghost"
          size="sm"
          onClick={add}
          disabled={disabled || columns.length === 0}
        >
          {t('transfer.mapping.addFilter')}
        </Button>
      </div>

      {conditions.length > 1 && (
        <label className="flex items-center gap-2 text-xs text-fg-muted">
          {t('transfer.mapping.filterLogic')}
          <Select
            data-testid="data-transfer-source-filter-logic"
            value={filter?.logic ?? 'and'}
            options={[
              { value: 'and', label: t('transfer.mapping.filterAll') },
              { value: 'or', label: t('transfer.mapping.filterAny') },
            ]}
            disabled={disabled}
            onChange={(logic) =>
              apply({
                filters: conditions,
                logic: logic as 'and' | 'or',
                ...(filter?.recordset ? { recordset: filter.recordset } : {}),
              })
            }
            className="!h-7 w-28 !text-xs"
          />
        </label>
      )}

      {conditions.length === 0 ? (
        <p className="text-xs text-fg-muted">{t('transfer.mapping.noSourceFilter')}</p>
      ) : (
        <div className="space-y-1">
          {conditions.map((condition, index) => {
            const needsValue =
              condition.operator !== 'isNull' && condition.operator !== 'isNotNull';
            return (
              <div
                key={`${condition.column}-${index}`}
                className="flex flex-wrap items-center gap-1"
              >
                <Select
                  value={condition.column}
                  options={columnOptions}
                  disabled={disabled}
                  onChange={(column) => {
                    const next = [...conditions];
                    next[index] = { ...condition, column };
                    update(next);
                  }}
                  className="!h-7 min-w-32 !text-xs"
                />
                <Select
                  value={condition.operator}
                  options={operatorOptions}
                  disabled={disabled}
                  onChange={(operator) => {
                    const next = [...conditions];
                    const nextOperator = operator as FilterOperator;
                    next[index] = {
                      ...condition,
                      operator: nextOperator,
                      value:
                        nextOperator === 'isNull' || nextOperator === 'isNotNull'
                          ? undefined
                          : (condition.value ?? ''),
                    };
                    update(next);
                  }}
                  className="!h-7 min-w-28 !text-xs"
                />
                {needsValue && (
                  <Input
                    value={condition.value == null ? '' : String(condition.value)}
                    placeholder={condition.operator === 'in' ? 'a,b,c' : t('filter.value')}
                    disabled={disabled}
                    onChange={(event) => {
                      const next = [...conditions];
                      next[index] = { ...condition, value: event.target.value };
                      update(next);
                    }}
                    className="h-7 min-w-32 flex-1 rounded border border-edge bg-surface px-2 text-xs"
                  />
                )}
                <Button
                  variant="ghost"
                  size="sm"
                  aria-label={t('filter.remove')}
                  disabled={disabled}
                  onClick={() => update(conditions.filter((_, itemIndex) => itemIndex !== index))}
                >
                  <X className="h-3.5 w-3.5" />
                </Button>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
