import { useEffect, useMemo, useRef, useState } from 'react';
import { useI18n } from '../../../hooks/useI18n';
import { Dialog } from '../../ui/Dialog';
import { Button } from '../../ui/Button';
import { Input } from '../../ui/Input';
import { Select, type SelectOption } from '../../ui/Select';
import type { QbAggregate, QbCondition, QbOperator } from '../types';
import { qualifiedRef } from './columnOptions';
import { getOperatorOptions } from '../operatorFilter';
import { TemporalValueInput } from '@datazen/ui';
import {
  isTemporalColumnType,
  normalizeTemporalValue,
  temporalInputType,
  validateTemporalValue,
} from '../temporalValue';

const AGGREGATE_VALUES: QbAggregate[] = ['COUNT', 'SUM', 'AVG', 'MIN', 'MAX'];
const NULL_OPERATORS = new Set<string>(['IS NULL', 'IS NOT NULL']);
const LOGIC_OPTIONS: SelectOption[] = [
  { value: 'AND', label: 'AND' },
  { value: 'OR', label: 'OR' },
];

/** One condition being edited (or created) in the dialog. */
export interface ConditionDraft {
  /** `null` while creating — nothing is written until OK. */
  id: string | null;
  groupId: string;
  condition: QbCondition;
  /** Hides the conjunction control on the first row of a group. */
  isFirstInGroup: boolean;
}

export interface ConditionDialogProps {
  draft: ConditionDraft | null;
  /** HAVING rows may wrap their operand in an aggregate; WHERE rows may not. */
  allowAggregate: boolean;
  allTables: string[];
  allColumns: Record<string, string[]>;
  /** Per-table column type map (table → column → raw dataType). */
  allColumnTypes?: Record<string, Record<string, string>>;
  tableAliases: Record<string, string>;
  onApply: (draft: ConditionDraft) => void;
  onRemove: (id: string) => void;
  onClose: () => void;
}

/**
 * One condition, edited in a dialog — the same "click the chip to configure it"
 * rule every other clause row follows.
 *
 * A new condition exists only as this draft: nothing reaches the store until OK,
 * so an abandoned draft cannot leave a half-filled condition behind that would
 * both render an empty chip and block OK with `empty-condition-value`.
 */
export function ConditionDialog({
  draft,
  allowAggregate,
  allTables,
  allColumns,
  allColumnTypes,
  tableAliases,
  onApply,
  onRemove,
  onClose,
}: ConditionDialogProps) {
  const { t } = useI18n();
  const [form, setForm] = useState<QbCondition | null>(null);

  const seedKey = draft ? `${draft.id ?? 'new'}:${draft.groupId}:${draft.condition.id}` : null;
  const seededRef = useRef<string | null>(null);

  useEffect(() => {
    if (!draft || !seedKey) {
      seededRef.current = null;
      setForm(null);
      return;
    }
    if (seededRef.current === seedKey) return;
    seededRef.current = seedKey;
    setForm(draft.condition);
  }, [draft, seedKey]);

  const fieldOptions: SelectOption[] = useMemo(() => {
    const options: SelectOption[] = [];
    for (const table of allTables) {
      for (const column of allColumns[table] ?? []) {
        options.push({
          value: `${table}.${column}`,
          label: qualifiedRef(table, column, tableAliases),
        });
      }
    }
    return options;
  }, [allTables, allColumns, tableAliases]);

  const aggregateOptions: SelectOption[] = [
    { value: '', label: t('query.visualBuilder.noAggregate') },
    ...AGGREGATE_VALUES.map((a) => ({ value: a, label: a })),
  ];

  const editing = !!draft?.id;
  const fieldValue = form ? `${form.table}.${form.column}` : '';
  const isNullOp = form ? NULL_OPERATORS.has(form.operator) : false;

  // The selected column's raw type drives the picker, the operator list and
  // the literal validation below.
  const colType = form ? allColumnTypes?.[form.table]?.[form.column] : undefined;
  const valueInputType = temporalInputType(colType);
  const isTemporalValue = isTemporalColumnType(colType);

  // A malformed temporal literal reaches the engine as
  // `operator does not exist: timestamp > integer` — block OK instead. An
  // empty temporal value blocks OK too: there is no sensible literal to send.
  const rawValue = form?.value ?? '';
  const temporalEmpty = isTemporalValue && !isNullOp && rawValue === '';
  const temporalInvalid =
    isTemporalValue &&
    !isNullOp &&
    rawValue !== '' &&
    validateTemporalValue(rawValue, colType) === 'invalid';

  // Filter operators based on the selected column's data type
  const operatorOptions: SelectOption[] = useMemo(() => {
    if (!form) return [];
    return getOperatorOptions(colType);
  }, [form, colType]);

  // Auto-reset operator when column changes and current operator is no longer valid
  useEffect(() => {
    if (!form) return;
    const valid = operatorOptions.some((o) => o.value === form.operator);
    if (!valid && form.operator) {
      setForm((f) => (f ? { ...f, operator: '=' as QbOperator } : f));
    }
  }, [form?.table, form?.column]);

  return (
    <Dialog
      open={!!draft && !!form}
      title={
        editing
          ? t('query.visualBuilder.conditionTitle')
          : t('query.visualBuilder.addConditionTitle')
      }
      onClose={onClose}
      testId="qb-condition-dialog"
      className="max-w-md"
      footer={
        <div className="flex w-full items-center justify-between gap-2">
          {editing && draft ? (
            <Button
              variant="ghost"
              size="sm"
              onClick={() => {
                onRemove(draft.id!);
                onClose();
              }}
              data-testid="qb-cond-remove"
            >
              {t('query.visualBuilder.removeJoin')}
            </Button>
          ) : (
            <span />
          )}
          <div className="flex items-center gap-2">
            <Button variant="ghost" size="sm" onClick={onClose} data-testid="qb-cond-cancel">
              {t('query.visualBuilder.cancel')}
            </Button>
            <Button
              variant="primary"
              size="sm"
              disabled={!form || temporalEmpty || temporalInvalid}
              onClick={() => {
                if (!form || !draft || temporalEmpty || temporalInvalid) return;
                const value = isTemporalValue
                  ? normalizeTemporalValue(form.value ?? '', colType)
                  : form.value;
                onApply({ ...draft, condition: { ...form, value } });
                onClose();
              }}
              data-testid="qb-cond-apply"
            >
              {t('query.visualBuilder.ok')}
            </Button>
          </div>
        </div>
      }
    >
      {form && (
        <div className="flex flex-col gap-3">
          <label className="flex items-center justify-between gap-2">
            <span className="text-[11px] text-fg-muted">{t('query.visualBuilder.fieldLabel')}</span>
            <div className="w-48">
              <Select
                value={fieldValue}
                options={fieldOptions}
                searchable
                placeholder="table.column"
                onChange={(v) => {
                  const dot = v.indexOf('.');
                  if (dot === -1) return;
                  setForm((f) =>
                    f ? { ...f, table: v.slice(0, dot), column: v.slice(dot + 1) } : f,
                  );
                }}
                triggerDataAttrs={{ 'data-testid': 'qb-cond-field' }}
              />
            </div>
          </label>

          {allowAggregate && (
            <label className="flex items-center justify-between gap-2">
              <span className="text-[11px] text-fg-muted">
                {t('query.visualBuilder.aggregate')}
              </span>
              <div className="w-48">
                <Select
                  value={form.aggregate ?? ''}
                  options={aggregateOptions}
                  onChange={(v) =>
                    setForm((f) =>
                      f ? { ...f, aggregate: (v || undefined) as QbAggregate | undefined } : f,
                    )
                  }
                  triggerDataAttrs={{ 'data-testid': 'qb-cond-aggregate' }}
                />
              </div>
            </label>
          )}

          <label className="flex items-center justify-between gap-2">
            <span className="text-[11px] text-fg-muted">
              {t('query.visualBuilder.operatorLabel')}
            </span>
            <div className="w-48">
              <Select
                value={form.operator}
                options={operatorOptions}
                onChange={(v) => setForm((f) => (f ? { ...f, operator: v as QbOperator } : f))}
                triggerDataAttrs={{ 'data-testid': 'qb-cond-operator' }}
              />
            </div>
          </label>

          <label className="flex items-center justify-between gap-2">
            <span className="text-[11px] text-fg-muted">
              {t('query.visualBuilder.valuePlaceholder')}
            </span>
            {isTemporalValue ? (
              <div className="w-48">
                <TemporalValueInput
                  kind={
                    valueInputType === 'time'
                      ? 'time'
                      : valueInputType === 'date'
                        ? 'date'
                        : 'datetime'
                  }
                  value={form.value ?? ''}
                  invalid={temporalInvalid}
                  disabled={isNullOp}
                  placeholder={t('query.visualBuilder.valuePlaceholder')}
                  onChange={(v) => setForm((f) => (f ? { ...f, value: v } : f))}
                  data-testid="qb-cond-value"
                />
              </div>
            ) : (
              <Input
                value={form.value ?? ''}
                disabled={isNullOp}
                type="text"
                onChange={(e) => setForm((f) => (f ? { ...f, value: e.target.value } : f))}
                placeholder={t('query.visualBuilder.valuePlaceholder')}
                className="h-8 w-48 text-xs"
                data-testid="qb-cond-value"
              />
            )}
          </label>
          {temporalInvalid && (
            <p className="text-right text-[11px] text-danger" data-testid="qb-cond-value-error">
              {t('query.visualBuilder.temporalFormatError')}
            </p>
          )}

          {!draft?.isFirstInGroup && (
            <label className="flex items-center justify-between gap-2">
              <span className="text-[11px] text-fg-muted">
                {t('query.visualBuilder.conjunctionLabel')}
              </span>
              <div className="w-48">
                <Select
                  value={form.conjunction}
                  options={LOGIC_OPTIONS}
                  onChange={(v) =>
                    setForm((f) => (f ? { ...f, conjunction: v as 'AND' | 'OR' } : f))
                  }
                  triggerDataAttrs={{ 'data-testid': 'qb-cond-conjunction' }}
                />
              </div>
            </label>
          )}
        </div>
      )}
    </Dialog>
  );
}
