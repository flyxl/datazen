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
}

export function SourceFilterEditor({ columns, filter, onChange }: SourceFilterEditorProps) {
  const { t } = useI18n();
  const conditions = filter?.filters ?? [];
  const columnOptions = columns.map((column) => ({ value: column, label: column }));
  const operatorOptions = OPERATORS.map((operator) => ({
    value: operator,
    label: t(`filter.${operator}`),
  }));

  const update = (next: FilterCondition[]) => {
    onChange(
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
        <Button variant="ghost" size="sm" onClick={add} disabled={columns.length === 0}>
          {t('transfer.mapping.addFilter')}
        </Button>
      </div>

      {conditions.length > 1 && (
        <label className="flex items-center gap-2 text-xs text-fg-muted">
          {t('transfer.mapping.filterLogic')}
          <Select
            value={filter?.logic ?? 'and'}
            options={[
              { value: 'and', label: t('transfer.mapping.filterAll') },
              { value: 'or', label: t('transfer.mapping.filterAny') },
            ]}
            onChange={(logic) =>
              onChange({
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
