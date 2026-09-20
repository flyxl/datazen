import { useMemo, useState } from 'react';
import { buildJoinSteps } from '../../../lib/sqlDialects/queryBuilder';
import { useI18n } from '../../../hooks/useI18n';
import type {
  QbAggregate,
  QbColumnSelection,
  QbCondition,
  QbConditionGroup,
  QbGroupByItem,
  QbJoin,
  QbSortItem,
} from '../types';
import { ClauseRow } from './ClauseRow';
import { Chip } from './Chip';
import { ColumnOptionsDialog } from './ColumnOptionsDialog';
import { ConditionChips } from './ConditionChips';
import { ConditionDialog, type ConditionDraft } from './ConditionDialog';
import { FromClause } from './FromClause';
import { GroupByClause } from './GroupByClause';
import { OrderByClause } from './OrderByClause';
import { SelectClause } from './SelectClause';
import { SortOptionsDialog } from './SortOptionsDialog';
import { TableOptionsDialog, type TableJoinInfo } from './TableOptionsDialog';
import { buildGroupByEntries, buildOrderByEntries, type ClauseEntry } from './clauseEntries';
import { qualifiedRef } from './columnOptions';
import { findGroupLogic, firstConditionOf } from './conditionText';
import { joinStepOnText, orderEntryLabel } from './joinText';

export interface BuildStatementSchema {
  /** Tables currently in the query. */
  tables: string[];
  /** table → column names. */
  columns: Record<string, string[]>;
  /** table → column → raw dataType. */
  columnTypes?: Record<string, Record<string, string>>;
  /** Table → alias. */
  aliases: Record<string, string>;
  /** Connection tables not in the query yet (FROM picker). */
  availableTables: string[];
}

export interface BuildStatementState {
  selectedColumns: QbColumnSelection[];
  distinct: boolean;
  joins: QbJoin[];
  where: QbConditionGroup;
  having: QbConditionGroup;
  groupBy: QbGroupByItem[];
  orderBy: QbSortItem[];
}

export type ConditionClauseKind = 'where' | 'having';

export interface BuildStatementActions {
  setDistinct: (v: boolean) => void;
  addColumn: (table: string, column: string) => void;
  removeColumn: (table: string, column: string) => void;
  updateColumn: (table: string, column: string, patch: Partial<QbColumnSelection>) => void;
  setTableAlias: (table: string, alias: string) => void;
  removeTable: (table: string) => void;
  addTable: (table: string) => void;
  addCondition: (groupId: string, condition: Omit<QbCondition, 'id'>) => void;
  updateCondition: (id: string, patch: Partial<QbCondition>) => void;
  removeCondition: (id: string) => void;
  addConditionGroup: (parentId: string, logic: 'AND' | 'OR') => void;
  setGroupLogic: (groupId: string, logic: 'AND' | 'OR') => void;
  addHavingCondition: (groupId: string, condition: Omit<QbCondition, 'id'>) => void;
  updateHavingCondition: (id: string, patch: Partial<QbCondition>) => void;
  removeHavingCondition: (id: string) => void;
  addHavingGroup: (parentId: string, logic: 'AND' | 'OR') => void;
  setHavingGroupLogic: (groupId: string, logic: 'AND' | 'OR') => void;
  addGroupBy: (table: string, column: string) => void;
  removeGroupBy: (entry: ClauseEntry) => void;
  addSort: (table: string, column: string) => void;
  /** Rewrite one ORDER BY entry's direction, wherever the entry lives. */
  setSort: (entry: ClauseEntry, direction: 'ASC' | 'DESC') => void;
  removeSort: (entry: ClauseEntry) => void;
}

export interface BuildStatementProps {
  schema: BuildStatementSchema;
  state: BuildStatementState;
  actions: BuildStatementActions;
}

/** Which condition dialog is open, and on what. */
type OpenCondition = { clause: ConditionClauseKind; draft: ConditionDraft };

/**
 * The Build tab, Navicat style: one row per SQL clause (SELECT / FROM / WHERE /
 * GROUP BY / HAVING / ORDER BY), and **one chip per item in every clause**.
 *
 * The uniform rule is the point: a chip shows what is in the query, clicking it
 * opens that item's options in a dialog, and its × removes it. Selecting a
 * column used to fill a whole grid row with eight inline controls, so four
 * columns hid WHERE and everything below it — and every other clause had its own
 * editing convention on top of that.
 */
export function BuildStatement({ schema, state, actions }: BuildStatementProps) {
  const { t } = useI18n();
  const [openColumn, setOpenColumn] = useState<{ table: string; column: string } | null>(null);
  const [openTable, setOpenTable] = useState<string | null>(null);
  const [openSort, setOpenSort] = useState<ClauseEntry | null>(null);
  const [openCondition, setOpenCondition] = useState<OpenCondition | null>(null);

  const groupByEntries = useMemo(
    () => buildGroupByEntries(state.groupBy, state.selectedColumns),
    [state.groupBy, state.selectedColumns],
  );
  const orderByEntries = useMemo(
    () => buildOrderByEntries(state.orderBy, state.selectedColumns),
    [state.orderBy, state.selectedColumns],
  );

  /** Per-column criteria are merged into WHERE by the generator — show them. */
  const perColumnCriteria = state.selectedColumns.filter((c) => c.where);

  const openSelection = openColumn
    ? (state.selectedColumns.find(
        (c) => c.table === openColumn.table && c.column === openColumn.column,
      ) ?? { table: openColumn.table, column: openColumn.column })
    : null;

  const joinSteps = useMemo(
    () => buildJoinSteps(state.joins, schema.tables[0]),
    [state.joins, schema.tables],
  );
  const joinInfoFor = (table: string): TableJoinInfo | null => {
    const step = joinSteps.find((s) => s.targetTable === table);
    return step ? { type: step.type, on: joinStepOnText(step, schema.aliases) } : null;
  };

  const treeOf = (clause: ConditionClauseKind) => (clause === 'where' ? state.where : state.having);

  const treeActions = (clause: ConditionClauseKind) =>
    clause === 'where'
      ? {
          add: actions.addCondition,
          update: actions.updateCondition,
          remove: actions.removeCondition,
          addGroup: actions.addConditionGroup,
          setLogic: actions.setGroupLogic,
        }
      : {
          add: actions.addHavingCondition,
          update: actions.updateHavingCondition,
          remove: actions.removeHavingCondition,
          addGroup: actions.addHavingGroup,
          setLogic: actions.setHavingGroupLogic,
        };

  /** A new condition exists only as a draft: nothing is written until OK. */
  const openNewCondition = (clause: ConditionClauseKind, groupId: string) => {
    const firstTable = schema.tables[0];
    const firstColumn = firstTable ? (schema.columns[firstTable] ?? [])[0] : undefined;
    if (!firstTable || !firstColumn) return;
    const tree = treeOf(clause);
    setOpenCondition({
      clause,
      draft: {
        id: null,
        groupId,
        isFirstInGroup: !firstConditionOf(tree, groupId),
        condition: {
          id: `draft-${clause}-${groupId}`,
          table: firstTable,
          column: firstColumn,
          operator: '=',
          value: '',
          // Seeded from the group's logic: the generator joins rows with each
          // row's own conjunction, so a hardcoded AND would turn an OR group
          // into an AND one.
          conjunction: findGroupLogic(tree, groupId) ?? 'AND',
          ...(clause === 'having' ? { aggregate: 'SUM' as QbAggregate } : {}),
        },
      },
    });
  };

  const openExistingCondition = (
    clause: ConditionClauseKind,
    condition: QbCondition,
    groupId: string,
  ) => {
    const tree = treeOf(clause);
    setOpenCondition({
      clause,
      draft: {
        id: condition.id,
        groupId,
        isFirstInGroup: firstConditionOf(tree, groupId)?.id === condition.id,
        condition,
      },
    });
  };

  const applyCondition = (draft: ConditionDraft) => {
    if (!openCondition) return;
    const handlers = treeActions(openCondition.clause);
    if (draft.id) {
      const { id: _id, ...patch } = draft.condition;
      handlers.update(draft.id, patch);
    } else {
      const { id: _placeholder, ...rest } = draft.condition;
      handlers.add(draft.groupId, rest);
    }
    setOpenCondition(null);
  };

  const renderConditionRow = (clause: ConditionClauseKind) => {
    const handlers = treeActions(clause);
    return (
      <ConditionChips
        group={treeOf(clause)}
        testIdPrefix={`qb-${clause}`}
        tableAliases={schema.aliases}
        allowAggregate={clause === 'having'}
        onAdd={(groupId) => openNewCondition(clause, groupId)}
        onOpen={(condition, groupId) => openExistingCondition(clause, condition, groupId)}
        onRemove={handlers.remove}
        onAddGroup={(parentId) => handlers.addGroup(parentId, 'OR')}
        onSetGroupLogic={handlers.setLogic}
      />
    );
  };

  return (
    <div className="flex min-w-0 flex-col divide-y divide-edge" data-testid="qb-statement">
      <ClauseRow label="SELECT" testId="qb-clause-select">
        <SelectClause
          selectedColumns={state.selectedColumns}
          allTables={schema.tables}
          allColumns={schema.columns}
          tableAliases={schema.aliases}
          distinct={state.distinct}
          onSetDistinct={actions.setDistinct}
          onOpenColumn={(table, column) => setOpenColumn({ table, column })}
          onRemoveColumn={actions.removeColumn}
          onAddColumn={actions.addColumn}
        />
      </ClauseRow>

      <ClauseRow label="FROM" testId="qb-clause-from">
        <FromClause
          selectedTables={schema.tables}
          tableAliases={schema.aliases}
          joins={state.joins}
          availableTables={schema.availableTables}
          onOpenTable={setOpenTable}
          onRemoveTable={actions.removeTable}
          onAddTable={actions.addTable}
        />
      </ClauseRow>

      <ClauseRow label="WHERE" testId="qb-clause-where">
        {perColumnCriteria.length > 0 && (
          <div
            className="flex min-w-0 flex-wrap items-center gap-1.5"
            data-testid="qb-where-column-chips"
          >
            {perColumnCriteria.map((col) => (
              <Chip
                key={`${col.table}.${col.column}`}
                label={qualifiedRef(col.table, col.column, schema.aliases)}
                suffix={` ${col.where!.operator}${col.where!.value ? ` ${col.where!.value}` : ''}`}
                muted
                title={t('query.visualBuilder.columnOptionsTitle')}
                testId={`qb-where-column-chip-${col.table}-${col.column}`}
                onClick={() => setOpenColumn({ table: col.table, column: col.column })}
                onRemove={() => actions.updateColumn(col.table, col.column, { where: undefined })}
                removeTestId={`qb-where-column-remove-${col.table}-${col.column}`}
              />
            ))}
          </div>
        )}
        {renderConditionRow('where')}
      </ClauseRow>

      <ClauseRow label="GROUP BY" testId="qb-clause-group-by">
        <GroupByClause
          items={groupByEntries}
          allTables={schema.tables}
          allColumns={schema.columns}
          tableAliases={schema.aliases}
          selectedKeys={state.selectedColumns.map((c) => `${c.table}.${c.column}`)}
          onAdd={actions.addGroupBy}
          onOpen={(entry) => setOpenColumn({ table: entry.table, column: entry.column })}
          onRemove={actions.removeGroupBy}
        />
      </ClauseRow>

      <ClauseRow
        label="HAVING"
        testId="qb-clause-having"
        title={t('query.visualBuilder.havingHint')}
      >
        {renderConditionRow('having')}
      </ClauseRow>

      <ClauseRow label="ORDER BY" testId="qb-clause-order-by">
        <OrderByClause
          entries={orderByEntries}
          allTables={schema.tables}
          allColumns={schema.columns}
          tableAliases={schema.aliases}
          onAdd={actions.addSort}
          onOpen={setOpenSort}
          onRemove={actions.removeSort}
        />
      </ClauseRow>

      <ColumnOptionsDialog
        selection={openSelection}
        columnType={
          openSelection
            ? schema.columnTypes?.[openSelection.table]?.[openSelection.column]
            : undefined
        }
        tableAliases={schema.aliases}
        onApply={actions.updateColumn}
        onRemove={actions.removeColumn}
        onClose={() => setOpenColumn(null)}
      />

      <TableOptionsDialog
        table={openTable}
        alias={openTable ? (schema.aliases[openTable] ?? '') : ''}
        join={openTable ? joinInfoFor(openTable) : null}
        onApply={(table, alias) => actions.setTableAlias(table, alias)}
        onRemove={actions.removeTable}
        onClose={() => setOpenTable(null)}
      />

      <SortOptionsDialog
        entry={openSort}
        label={
          openSort
            ? orderEntryLabel(openSort.table, openSort.column, openSort.aggregate, schema.aliases)
            : ''
        }
        onApply={(entry, direction) => actions.setSort(entry, direction)}
        onRemove={actions.removeSort}
        onClose={() => setOpenSort(null)}
      />

      <ConditionDialog
        draft={openCondition?.draft ?? null}
        allowAggregate={openCondition?.clause === 'having'}
        allTables={schema.tables}
        allColumns={schema.columns}
        allColumnTypes={schema.columnTypes}
        tableAliases={schema.aliases}
        onApply={applyCondition}
        onRemove={(id) => {
          if (!openCondition) return;
          treeActions(openCondition.clause).remove(id);
          setOpenCondition(null);
        }}
        onClose={() => setOpenCondition(null)}
      />
    </div>
  );
}
