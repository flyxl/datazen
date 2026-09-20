import { useEffect, useMemo, useRef, useState } from 'react';
import { useI18n } from '../../../hooks/useI18n';
import { Dialog } from '../../ui/Dialog';
import { Button } from '../../ui/Button';
import { Input } from '../../ui/Input';
import { Select, type SelectOption } from '../../ui/Select';
import type { QbAggregate, QbColumnSelection, QbOperator } from '../types';
import { qualifiedRef } from './columnOptions';
import { getOperatorOptions } from '../operatorFilter';

const AGGREGATE_VALUES: QbAggregate[] = ['COUNT', 'SUM', 'AVG', 'MIN', 'MAX'];

/** Operators that take no right-hand value. */
const NULL_OPERATORS = new Set<string>(['IS NULL', 'IS NOT NULL']);

interface FormState {
  alias: string;
  aggregate: QbAggregate | '';
  sort: 'ASC' | 'DESC' | '';
  groupBy: boolean;
  operator: QbOperator | '';
  value: string;
}

const EMPTY_FORM: FormState = {
  alias: '',
  aggregate: '',
  sort: '',
  groupBy: false,
  operator: '',
  value: '',
};

export interface ColumnOptionsDialogProps {
  /** The column being edited; `null` keeps the dialog closed. */
  selection: QbColumnSelection | null;
  /** Raw dataType of the selected column (for operator filtering). */
  columnType?: string;
  tableAliases: Record<string, string>;
  onApply: (table: string, column: string, patch: Partial<QbColumnSelection>) => void;
  onRemove: (table: string, column: string) => void;
  onClose: () => void;
}

/**
 * Every per-field option of one selected column, in one popup.
 *
 * This is the change that bought the Build tab its vertical space: the old
 * layout put field / table / alias / sort / function / where / group on one
 * grid row per column, so four selected columns consumed the whole region and
 * WHERE, GROUP BY, HAVING and ORDER BY fell off the bottom. Here a column is a
 * single chip, and its options appear only when asked for.
 *
 * The form is seeded once per opened column (not on every store change), so an
 * unrelated store update cannot wipe what the user is typing.
 */
export function ColumnOptionsDialog({
  selection,
  columnType,
  tableAliases,
  onApply,
  onRemove,
  onClose,
}: ColumnOptionsDialogProps) {
  const { t } = useI18n();
  const [form, setForm] = useState<FormState>(EMPTY_FORM);

  const key = selection ? `${selection.table}.${selection.column}` : null;
  const seededRef = useRef<string | null>(null);

  useEffect(() => {
    if (!key || !selection) {
      seededRef.current = null;
      return;
    }
    if (seededRef.current === key) return;
    seededRef.current = key;
    setForm({
      alias: selection.alias ?? '',
      aggregate: selection.aggregate ?? '',
      sort: selection.sort ?? '',
      groupBy: selection.groupBy ?? false,
      operator: selection.where?.operator ?? '',
      value: selection.where?.value ?? '',
    });
  }, [key, selection]);

  const aggregateOptions: SelectOption[] = [
    { value: '', label: t('query.visualBuilder.noAggregate') },
    ...AGGREGATE_VALUES.map((a) => ({ value: a, label: a })),
  ];
  const sortOptions: SelectOption[] = [
    { value: '', label: t('query.visualBuilder.noSort') },
    { value: 'ASC', label: t('query.visualBuilder.asc') },
    { value: 'DESC', label: t('query.visualBuilder.desc') },
  ];
  const operatorOptions: SelectOption[] = useMemo(
    () => [{ value: '', label: '—' }, ...getOperatorOptions(columnType)],
    [columnType],
  );

  const handleApply = () => {
    if (!selection) return;
    onApply(selection.table, selection.column, {
      alias: form.alias.trim() || undefined,
      aggregate: form.aggregate || undefined,
      sort: form.sort || undefined,
      groupBy: form.groupBy || undefined,
      where: form.operator
        ? {
            id: selection.where?.id ?? `col-where-${key}`,
            table: selection.table,
            column: selection.column,
            operator: form.operator,
            value: form.value,
            conjunction: 'AND',
          }
        : undefined,
    });
    onClose();
  };

  return (
    <Dialog
      open={!!selection}
      title={t('query.visualBuilder.columnOptionsTitle')}
      onClose={onClose}
      testId="qb-column-options"
      className="max-w-md"
      footer={
        <div className="flex w-full items-center justify-between gap-2">
          <Button
            variant="ghost"
            size="sm"
            onClick={() => {
              if (!selection) return;
              onRemove(selection.table, selection.column);
              onClose();
            }}
            data-testid="qb-col-opt-remove"
          >
            {t('query.visualBuilder.removeColumn')}
          </Button>
          <div className="flex items-center gap-2">
            <Button variant="ghost" size="sm" onClick={onClose} data-testid="qb-col-opt-cancel">
              {t('query.visualBuilder.cancel')}
            </Button>
            <Button
              variant="primary"
              size="sm"
              onClick={handleApply}
              data-testid="qb-col-opt-apply"
            >
              {t('query.visualBuilder.ok')}
            </Button>
          </div>
        </div>
      }
    >
      <div className="flex flex-col gap-3">
        <div className="flex items-center justify-between gap-2">
          <span className="text-[11px] text-fg-muted">{t('query.visualBuilder.fieldLabel')}</span>
          <span className="truncate font-mono text-[12px] text-fg" data-testid="qb-col-opt-field">
            {selection ? qualifiedRef(selection.table, selection.column, tableAliases) : ''}
          </span>
        </div>

        <label className="flex items-center justify-between gap-2">
          <span className="text-[11px] text-fg-muted">{t('query.visualBuilder.alias')}</span>
          <Input
            value={form.alias}
            onChange={(e) => setForm((f) => ({ ...f, alias: e.target.value }))}
            className="h-8 w-40 text-xs"
            data-testid="qb-col-opt-alias"
          />
        </label>

        <label className="flex items-center justify-between gap-2">
          <span className="text-[11px] text-fg-muted">{t('query.visualBuilder.aggregate')}</span>
          <div className="w-40">
            <Select
              value={form.aggregate}
              options={aggregateOptions}
              onChange={(v) => setForm((f) => ({ ...f, aggregate: v as QbAggregate | '' }))}
              triggerDataAttrs={{ 'data-testid': 'qb-col-opt-aggregate' }}
            />
          </div>
        </label>

        <label className="flex items-center justify-between gap-2">
          <span className="text-[11px] text-fg-muted">{t('query.visualBuilder.sortLabel')}</span>
          <div className="w-40">
            <Select
              value={form.sort}
              options={sortOptions}
              onChange={(v) => setForm((f) => ({ ...f, sort: v as 'ASC' | 'DESC' | '' }))}
              triggerDataAttrs={{ 'data-testid': 'qb-col-opt-sort' }}
            />
          </div>
        </label>

        <label className="flex items-center justify-between gap-2">
          <span className="text-[11px] text-fg-muted">{t('query.visualBuilder.groupByLabel')}</span>
          <input
            type="checkbox"
            checked={form.groupBy}
            onChange={(e) => setForm((f) => ({ ...f, groupBy: e.target.checked }))}
            className="accent-accent h-4 w-4"
            data-testid="qb-col-opt-groupby"
          />
        </label>

        <div className="flex flex-col gap-2 border-t border-edge pt-3">
          <span className="text-[11px] text-fg-muted">
            {t('query.visualBuilder.criteriaLabel')}
          </span>
          <div className="flex items-center gap-2">
            <div className="w-32">
              <Select
                value={form.operator}
                options={operatorOptions}
                onChange={(v) => setForm((f) => ({ ...f, operator: v as QbOperator | '' }))}
                triggerDataAttrs={{ 'data-testid': 'qb-col-opt-operator' }}
              />
            </div>
            <Input
              value={form.value}
              onChange={(e) => setForm((f) => ({ ...f, value: e.target.value }))}
              disabled={!form.operator || NULL_OPERATORS.has(form.operator)}
              placeholder={t('query.visualBuilder.valuePlaceholder')}
              className="h-8 flex-1 text-xs"
              data-testid="qb-col-opt-value"
            />
          </div>
        </div>
      </div>
    </Dialog>
  );
}
