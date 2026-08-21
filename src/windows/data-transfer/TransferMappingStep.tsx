import { useEffect } from 'react';
import { cn } from '../../lib/cn';
import { useI18n } from '../../hooks/useI18n';
import type { TransferMode, TransferTableResult } from '../../commands/transfer';
import { ColumnMappingEditor } from './ColumnMappingEditor';

interface TransferMappingStepProps {
  tables: TransferTableResult[];
  selectedSourceTable: string;
  mode: TransferMode;
  onSelectTable: (sourceTable: string) => void;
  onUpdateTable: (sourceTable: string, patch: Partial<TransferTableResult>) => void;
  onTargetTableCommit: (sourceTable: string) => void;
}

export function TransferMappingStep({
  tables,
  selectedSourceTable,
  mode,
  onSelectTable,
  onUpdateTable,
  onTargetTableCommit,
}: TransferMappingStepProps) {
  const { t } = useI18n();
  const enabledTables = tables.filter((tbl) => tbl.enabled && tbl.sourceTable);
  const selected =
    enabledTables.find((tbl) => tbl.sourceTable === selectedSourceTable) ?? enabledTables[0];

  useEffect(() => {
    if (enabledTables.length === 0) return;
    if (!enabledTables.some((tbl) => tbl.sourceTable === selectedSourceTable)) {
      onSelectTable(enabledTables[0].sourceTable);
    }
  }, [enabledTables, selectedSourceTable, onSelectTable]);

  const structureMode = mode === 'structure' || mode === 'structureAndData';

  if (enabledTables.length === 0) {
    return (
      <p className="text-sm text-fg-muted" data-testid="data-transfer-mapping-empty">
        {t('transfer.mapping.noTables')}
      </p>
    );
  }

  return (
    <div className="flex min-h-0 flex-1 gap-4" data-testid="data-transfer-mapping-step">
      <div className="flex w-56 shrink-0 flex-col gap-1 overflow-auto rounded-lg border border-border bg-surface">
        <div className="border-b border-border px-3 py-1.5 text-[11px] font-semibold uppercase tracking-wider text-fg-muted">
          {t('transfer.mapping.tables')}
        </div>
        {enabledTables.map((tbl) => (
          <button
            key={tbl.sourceTable}
            type="button"
            data-testid="data-transfer-mapping-table-item"
            className={cn(
              'px-3 py-2 text-left text-sm hover:bg-surface-alt',
              selected?.sourceTable === tbl.sourceTable && 'bg-surface-alt font-medium text-accent',
            )}
            onClick={() => onSelectTable(tbl.sourceTable)}
          >
            <div className="truncate font-mono text-xs">{tbl.sourceTable}</div>
            <div className="truncate text-[11px] text-fg-muted">→ {tbl.targetTable || '—'}</div>
          </button>
        ))}
      </div>

      {selected && (
        <ColumnMappingEditor
          key={selected.sourceTable}
          table={selected}
          structureMode={structureMode}
          onChange={(patch) => onUpdateTable(selected.sourceTable, patch)}
          onTargetTableBlur={() => onTargetTableCommit(selected.sourceTable)}
        />
      )}
    </div>
  );
}
