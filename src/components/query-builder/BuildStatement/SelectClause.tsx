import { useMemo } from 'react';
import { useI18n } from '../../../hooks/useI18n';
import type { QbColumnSelection } from '../types';
import { Chip } from './Chip';
import { LinkSelect } from './LinkSelect';
import { buildColumnOptions, fieldChipParts, parseFieldKey } from './columnOptions';

export interface SelectClauseProps {
  selectedColumns: QbColumnSelection[];
  allTables: string[];
  allColumns: Record<string, string[]>;
  tableAliases: Record<string, string>;
  distinct: boolean;
  onSetDistinct: (v: boolean) => void;
  /** Click a field chip → open its options dialog. */
  onOpenColumn: (table: string, column: string) => void;
  onRemoveColumn: (table: string, column: string) => void;
  onAddColumn: (table: string, column: string) => void;
}

/**
 * The SELECT row: `<Distinct>` plus one chip per selected field, each chip
 * carrying that field's aggregate and alias — and a link that opens the field
 * picker.
 *
 * Per-field configuration (alias, aggregate, sort, group-by, criteria) is *not*
 * inline any more: it moved into the options dialog a chip click opens. That is
 * what took the clause from a 8-column table row to a single chip.
 */
export function SelectClause({
  selectedColumns,
  allTables,
  allColumns,
  tableAliases,
  distinct,
  onSetDistinct,
  onOpenColumn,
  onRemoveColumn,
  onAddColumn,
}: SelectClauseProps) {
  const { t } = useI18n();

  const options = useMemo(
    () => buildColumnOptions(allTables, allColumns, tableAliases),
    [allTables, allColumns, tableAliases],
  );
  const selectedKeys = new Set(selectedColumns.map((c) => `${c.table}.${c.column}`));
  const available = options.filter((o) => !selectedKeys.has(o.value));

  return (
    <div className="flex min-w-0 flex-wrap items-center gap-1.5" data-testid="qb-select-clause">
      <label
        className="flex shrink-0 items-center gap-1 text-[11px] text-fg-muted"
        title={t('query.visualBuilder.distinct')}
      >
        <input
          type="checkbox"
          checked={distinct}
          onChange={(e) => onSetDistinct(e.target.checked)}
          className="accent-accent h-3.5 w-3.5"
          data-testid="qb-distinct-checkbox"
        />
        DISTINCT
      </label>

      {selectedColumns.map((selection) => {
        const { label, prefix, suffix } = fieldChipParts(selection, tableAliases);
        const key = `${selection.table}.${selection.column}`;
        return (
          <Chip
            key={key}
            label={label}
            prefix={prefix}
            suffix={suffix}
            testId={`qb-field-chip-${selection.table}-${selection.column}`}
            title={t('query.visualBuilder.columnOptionsTitle')}
            onClick={() => onOpenColumn(selection.table, selection.column)}
            onRemove={() => onRemoveColumn(selection.table, selection.column)}
            removeTestId={`qb-field-remove-${selection.table}-${selection.column}`}
            removeTitle={t('query.visualBuilder.removeColumn')}
          />
        );
      })}

      <LinkSelect
        label={t('query.visualBuilder.addFields')}
        options={available}
        testId="qb-add-fields"
        onPick={(key) => {
          const field = parseFieldKey(key);
          if (field) onAddColumn(field.table, field.column);
        }}
      />
    </div>
  );
}
