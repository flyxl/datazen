import { useMemo } from 'react';
import { useI18n } from '../../../hooks/useI18n';
import { Chip } from './Chip';
import { LinkSelect } from './LinkSelect';
import { buildColumnOptions, parseFieldKey, qualifiedRef } from './columnOptions';
import type { ClauseEntry } from './clauseEntries';

export interface GroupByClauseProps {
  items: ClauseEntry[];
  allTables: string[];
  allColumns: Record<string, string[]>;
  tableAliases: Record<string, string>;
  /** Columns that are also in SELECT — only those have options to open. */
  selectedKeys: string[];
  onAdd: (table: string, column: string) => void;
  /** Click a chip → that column's options (alias / aggregate / criteria). */
  onOpen: (entry: ClauseEntry) => void;
  onRemove: (entry: ClauseEntry) => void;
}

/**
 * GROUP BY row. The items are unioned from the store-level list and the
 * per-column `groupBy` flags, so a column marked "group by" in its options
 * dialog shows up here — and removing it here clears whichever source owns it
 * (the panel decides that, see `handleRemoveGroupBy`).
 */
export function GroupByClause({
  items,
  allTables,
  allColumns,
  tableAliases,
  selectedKeys,
  onAdd,
  onOpen,
  onRemove,
}: GroupByClauseProps) {
  const { t } = useI18n();

  const options = useMemo(
    () => buildColumnOptions(allTables, allColumns, tableAliases),
    [allTables, allColumns, tableAliases],
  );
  const used = new Set(items.map((i) => `${i.table}.${i.column}`));
  const openable = new Set(selectedKeys);

  return (
    <div className="flex min-w-0 flex-wrap items-center gap-1.5" data-testid="qb-groupby-clause">
      {items.map((item) => {
        const key = `${item.table}.${item.column}`;
        return (
          <Chip
            key={key}
            label={qualifiedRef(item.table, item.column, tableAliases)}
            testId={`qb-group-chip-${item.table}-${item.column}`}
            removeTestId={`qb-group-remove-${item.table}-${item.column}`}
            // A GROUP BY key has no options of its own: clicking it opens the
            // column's dialog, which exists only when the column is in SELECT.
            onClick={openable.has(key) ? () => onOpen(item) : undefined}
            title={openable.has(key) ? t('query.visualBuilder.columnOptionsTitle') : undefined}
            onRemove={() => onRemove(item)}
          />
        );
      })}
      <LinkSelect
        label={t('query.visualBuilder.addGroupByClause')}
        options={options.filter((o) => !used.has(o.value))}
        testId="qb-add-group-by"
        onPick={(key) => {
          const field = parseFieldKey(key);
          if (field) onAdd(field.table, field.column);
        }}
      />
    </div>
  );
}
