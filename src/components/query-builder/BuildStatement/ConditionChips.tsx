import { useI18n } from '../../../hooks/useI18n';
import { Select, type SelectOption } from '../../ui/Select';
import type { QbCondition, QbConditionGroup } from '../types';
import { Chip } from './Chip';
import { conditionChipParts, conditionChipPrefix } from './conditionText';

const LOGIC_OPTIONS: SelectOption[] = [
  { value: 'AND', label: 'AND' },
  { value: 'OR', label: 'OR' },
];

export interface ConditionChipsProps {
  /** Root group of the clause (the store's `where` or `having`). */
  group: QbConditionGroup;
  /** Test-id namespace — `qb-where` or `qb-having`. */
  testIdPrefix: string;
  /** Only used to label the chips with the qualifier the SQL will emit. */
  tableAliases: Record<string, string>;
  /** HAVING filters aggregates, so its dialog exposes an aggregate selector. */
  allowAggregate?: boolean;
  /** Placeholder when the clause holds nothing. */
  emptyHint?: string;
  /** Open the condition dialog for a *new* row of `groupId`. */
  onAdd: (groupId: string) => void;
  /** Open the condition dialog for an existing row. */
  onOpen: (condition: QbCondition, groupId: string) => void;
  onRemove: (id: string) => void;
  onAddGroup: (parentId: string) => void;
  onSetGroupLogic: (groupId: string, logic: 'AND' | 'OR') => void;
}

/**
 * A condition clause (WHERE or HAVING) rendered as chips.
 *
 * One condition = one chip: it shows the operand, the operator and the value,
 * with the row's `AND` / `OR` as a badge on every row but the first. Clicking it
 * opens the condition dialog; a nested group is a bordered box whose own logic
 * selector sits in front of its chips.
 *
 * This is the same rule the SELECT / FROM / GROUP BY / ORDER BY rows follow, so
 * the whole statement reads as one list of items instead of six different
 * editing conventions.
 */
export function ConditionChips({
  group,
  testIdPrefix,
  tableAliases,
  allowAggregate = false,
  emptyHint,
  onAdd,
  onOpen,
  onRemove,
  onAddGroup,
  onSetGroupLogic,
}: ConditionChipsProps) {
  const { t } = useI18n();
  const isEmpty = group.conditions.length === 0 && group.groups.length === 0;

  const renderChips = (target: QbConditionGroup, isRoot: boolean) => (
    <>
      {target.conditions.map((condition, index) => {
        const parts = conditionChipParts(condition, tableAliases);
        return (
          <Chip
            key={condition.id}
            badge={conditionChipPrefix(condition, index === 0) || undefined}
            label={parts.label}
            prefix={parts.prefix}
            suffix={parts.suffix}
            testId={`${testIdPrefix}-chip-${condition.id}`}
            title={t('query.visualBuilder.conditionTitle')}
            onClick={() => onOpen(condition, target.id)}
            onRemove={() => onRemove(condition.id)}
            removeTestId={`${testIdPrefix}-chip-remove-${condition.id}`}
          />
        );
      })}
      {target.groups.map((sub) => (
        <span
          key={sub.id}
          className="inline-flex min-w-0 flex-wrap items-center gap-1.5 rounded-[7px] border border-edge px-1.5 py-0.5"
          data-testid={`${testIdPrefix}-group`}
          data-group-id={sub.id}
          data-group-logic={sub.logic}
        >
          <span className="text-[11px] text-fg-muted">(</span>
          <div className="w-[68px] shrink-0">
            <Select
              value={sub.logic}
              options={LOGIC_OPTIONS}
              onChange={(v) => onSetGroupLogic(sub.id, v as 'AND' | 'OR')}
              triggerDataAttrs={{ 'data-testid': `${testIdPrefix}-group-logic` }}
            />
          </div>
          <span className="text-[11px] text-fg-muted">)</span>
          {renderChips(sub, false)}
          <button
            type="button"
            onClick={() => onAdd(sub.id)}
            className="rounded text-[11px] text-fg-muted hover:text-accent"
            data-testid={`${testIdPrefix}-subgroup-add-condition`}
          >
            +
          </button>
        </span>
      ))}
      {!isRoot && target.conditions.length === 0 && target.groups.length === 0 && (
        <span className="text-[11px] text-fg-muted">{emptyHint}</span>
      )}
    </>
  );

  return (
    <div className="flex min-w-0 flex-col gap-1" data-testid={`${testIdPrefix}-editor`}>
      <div
        className="flex min-w-0 flex-wrap items-center gap-1.5"
        data-testid={`${testIdPrefix}-root`}
        data-group-id={group.id}
        data-group-logic={group.logic}
      >
        {renderChips(group, true)}
      </div>

      <div className="flex items-center gap-3">
        <button
          type="button"
          onClick={() => onAdd(group.id)}
          className="rounded text-[12px] text-fg-muted transition-colors hover:text-accent"
          data-testid={`${testIdPrefix}-add-condition`}
        >
          {isEmpty ? (
            <span data-testid={`${testIdPrefix}-empty`}>
              &lt;{emptyHint ?? t('query.visualBuilder.addConditions')}&gt;
            </span>
          ) : (
            <>+ {t('query.visualBuilder.addCondition')}</>
          )}
        </button>
        <button
          type="button"
          onClick={() => onAddGroup(group.id)}
          className="rounded text-[12px] text-fg-muted transition-colors hover:text-accent"
          data-testid={`${testIdPrefix}-add-group`}
        >
          + {t('query.visualBuilder.addGroup')}
        </button>
      </div>

      {/* The aggregate selector only exists inside the HAVING dialog; keeping a
          named hook here makes the difference explicit in tests and E2E. */}
      {allowAggregate && <span hidden data-testid={`${testIdPrefix}-supports-aggregate`} />}
    </div>
  );
}
