import { Plus, X } from 'lucide-react';
import type { FilterCondition, FilterOperator } from '../types';
import { useI18n } from '../hooks/useI18n';
import { Select } from './ui/Select';
import { Input } from './ui/Input';
import { Button } from './ui/Button';
import { cn } from '../lib/cn';

export type FilterLogic = 'and' | 'or';

const OPERATORS: FilterOperator[] = [
  'eq', 'ne', 'gt', 'lt', 'gte', 'lte', 'like', 'in', 'isNull', 'isNotNull',
];

interface FilterEditorProps {
  columns: { name: string }[];
  filters: FilterCondition[];
  logic: FilterLogic;
  onLogicChange: (logic: FilterLogic) => void;
  onChange: (index: number, next: FilterCondition) => void;
  onAdd: (filter: FilterCondition) => void;
  onRemove: (index: number) => void;
  onClear: () => void;
}

export function FilterEditor({
  columns,
  filters,
  logic,
  onLogicChange,
  onChange,
  onAdd,
  onRemove,
  onClear,
}: FilterEditorProps) {
  const { t } = useI18n();
  const columnOptions = columns.map((c) => ({ value: c.name, label: c.name }));
  const opOptions = OPERATORS.map((op) => ({ value: op, label: t(`filter.${op}`) }));

  return (
    <div className="flex flex-col gap-2 border-b border-edge bg-surface px-3 py-2">
      <div className="flex items-center gap-2">
        <div className="text-[11px] font-semibold uppercase tracking-wider text-fg-muted">
          {t('filter.filter')}
        </div>
        <div className="inline-flex rounded-md border border-edge">
          <button
            type="button"
            className={cn(
              'px-2 py-0.5 text-[11px]',
              logic === 'and' ? 'bg-accent/15 text-accent' : 'text-fg-secondary',
            )}
            onClick={() => onLogicChange('and')}
          >
            {t('filter.and')}
          </button>
          <button
            type="button"
            className={cn(
              'px-2 py-0.5 text-[11px]',
              logic === 'or' ? 'bg-accent/15 text-accent' : 'text-fg-secondary',
            )}
            onClick={() => onLogicChange('or')}
          >
            {t('filter.or')}
          </button>
        </div>
        <Button
          variant="ghost"
          className="ml-auto h-6 gap-1 px-2 text-xs"
          onClick={() =>
            onAdd({
              column: columns[0]?.name ?? '',
              operator: 'eq',
              value: '',
            })
          }
          disabled={columns.length === 0}
        >
          <Plus className="h-3 w-3" />
          {t('filter.add')}
        </Button>
        {filters.length > 0 && (
          <button type="button" className="text-xs text-fg-secondary hover:text-fg" onClick={onClear}>
            {t('filter.clear')}
          </button>
        )}
      </div>
      {filters.map((f, idx) => {
        const needsValue = f.operator !== 'isNull' && f.operator !== 'isNotNull';
        return (
          <div key={`${f.column}-${idx}`} className="flex flex-wrap items-center gap-2">
            <Select
              value={f.column}
              options={columnOptions}
              onChange={(column) => onChange(idx, { ...f, column })}
              className="!h-7 min-w-[120px] !text-xs"
            />
            <Select
              value={f.operator}
              options={opOptions}
              onChange={(operator) => onChange(idx, { ...f, operator: operator as FilterOperator })}
              className="!h-7 min-w-[96px] !text-xs"
            />
            {needsValue && (
              <Input
                value={f.value == null ? '' : String(f.value)}
                onChange={(e) => onChange(idx, { ...f, value: e.target.value })}
                placeholder={t('filter.value')}
                className="h-7 w-40 text-xs"
              />
            )}
            <button
              type="button"
              className="rounded p-1 text-fg-muted hover:bg-surface-raised hover:text-fg"
              onClick={() => onRemove(idx)}
              aria-label={t('filter.remove')}
            >
              <X className="h-3.5 w-3.5" />
            </button>
          </div>
        );
      })}
    </div>
  );
}
