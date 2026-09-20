import { useMemo } from 'react';
import { useI18n } from '../../../hooks/useI18n';
import { buildJoinSteps } from '../../../lib/sqlDialects/queryBuilder';
import type { QbJoin } from '../types';
import { Chip } from './Chip';
import { LinkSelect } from './LinkSelect';
import { joinStepOnText } from './joinText';

export interface FromClauseProps {
  selectedTables: string[];
  tableAliases: Record<string, string>;
  joins: QbJoin[];
  /** Tables of the connection that are not in the query yet. */
  availableTables: string[];
  /** Click a table chip → its options (alias, and how it joins). */
  onOpenTable: (table: string) => void;
  onRemoveTable: (table: string) => void;
  onAddTable: (table: string) => void;
}

/**
 * The FROM row: one chip per table, exactly like one chip per column.
 *
 * The driving table and every joined table are the same object — a chip whose
 * click opens its options and whose × removes it — so the statement has one
 * interaction rule instead of a special case per clause.
 *
 * The join chips come from `buildJoinSteps`, the same graph walk the SQL
 * generator uses, so the order, the orientation of each `ON` predicate and the
 * merging of composite keys are identical to the emitted statement by
 * construction. The `ON` text itself lives in the chip's tooltip and its dialog;
 * `data-join-on` carries it for tests and E2E.
 */
export function FromClause({
  selectedTables,
  tableAliases,
  joins,
  availableTables,
  onOpenTable,
  onRemoveTable,
  onAddTable,
}: FromClauseProps) {
  const { t } = useI18n();

  const fromTable = selectedTables[0];
  const steps = useMemo(() => buildJoinSteps(joins, fromTable), [joins, fromTable]);

  const included = useMemo(() => {
    const set = new Set<string>();
    if (fromTable) set.add(fromTable);
    for (const step of steps) set.add(step.targetTable);
    return set;
  }, [fromTable, steps]);

  const unjoined = selectedTables.filter((table) => table !== fromTable && !included.has(table));

  const aliasSuffix = (table: string) =>
    tableAliases[table] && tableAliases[table] !== table ? ` AS ${tableAliases[table]}` : '';

  return (
    <div className="flex min-w-0 flex-wrap items-center gap-1.5" data-testid="qb-from-clause">
      {fromTable && (
        <Chip
          label={fromTable}
          suffix={aliasSuffix(fromTable)}
          testId={`qb-from-chip-${fromTable}`}
          title={t('query.visualBuilder.tableOptionsTitle')}
          onClick={() => onOpenTable(fromTable)}
          onRemove={() => onRemoveTable(fromTable)}
          removeTestId={`qb-from-remove-${fromTable}`}
          removeTitle={t('query.visualBuilder.removeTable')}
        />
      )}

      {steps.map((step, index) => {
        const on = joinStepOnText(step, tableAliases);
        return (
          <span
            key={`${step.targetTable}-${index}`}
            className="inline-flex min-w-0 items-center"
            data-testid={`qb-from-join-${index}`}
            data-join-target={step.targetTable}
            data-join-type={step.type}
            data-join-on={on}
            data-join-detached={step.detached ? 'true' : undefined}
          >
            <Chip
              badge={`${step.type} JOIN`}
              label={step.targetTable}
              suffix={aliasSuffix(step.targetTable)}
              testId={`qb-from-chip-${step.targetTable}`}
              title={`${t('query.visualBuilder.joinOn')} ${on}`}
              onClick={() => onOpenTable(step.targetTable)}
              onRemove={() => onRemoveTable(step.targetTable)}
              removeTestId={`qb-from-remove-${step.targetTable}`}
              removeTitle={t('query.visualBuilder.removeTable')}
            />
          </span>
        );
      })}

      {unjoined.map((table) => (
        <span
          key={table}
          className="inline-flex min-w-0 items-center gap-1.5"
          data-testid={`qb-from-unjoined-${table}`}
        >
          <Chip
            label={table}
            suffix={aliasSuffix(table)}
            muted
            testId={`qb-from-chip-${table}`}
            title={t('query.visualBuilder.unjoinedTable')}
            onClick={() => onOpenTable(table)}
            onRemove={() => onRemoveTable(table)}
            removeTestId={`qb-from-remove-${table}`}
            removeTitle={t('query.visualBuilder.removeTable')}
          />
          <span className="text-[11px] text-warning">{t('query.visualBuilder.unjoinedTable')}</span>
        </span>
      ))}

      <LinkSelect
        label={t('query.visualBuilder.addTables')}
        options={availableTables.map((table) => ({ value: table, label: table }))}
        testId="qb-add-tables"
        onPick={onAddTable}
      />
    </div>
  );
}
