import { Button } from '../../components/ui/Button';
import { Select } from '../../components/ui/Select';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';
import type { TransferColumnMapping, TransferTableResult } from '../../commands/transfer';
import {
  autoMatchColumnMappings,
  clearUnmappedColumnMappings,
  normalizeColumnMappings,
  unmappedTargetColumns,
} from './transferMappingView';

interface ColumnMappingEditorProps {
  table: TransferTableResult;
  structureMode: boolean;
  onChange: (patch: Partial<TransferTableResult>) => void;
  onTargetTableBlur?: () => void;
}

export function ColumnMappingEditor({
  table,
  structureMode,
  onChange,
  onTargetTableBlur,
}: ColumnMappingEditorProps) {
  const { t } = useI18n();
  const mappings = normalizeColumnMappings(table);
  const unmappedTargets = unmappedTargetColumns(table);
  const showCreateNewToggle = structureMode || table.status === 'CREATE_NEW' || table.createNew;
  const showTargetType = structureMode && table.createNew;

  const updateMappings = (next: TransferColumnMapping[]) => {
    onChange({ columnMappings: next });
  };

  const handleAutoMatch = () => {
    const sourceColumns = table.sourceColumns ?? mappings.map((m) => m.sourceColumn);
    const targetColumns = table.createNew ? sourceColumns : (table.targetColumns ?? []);
    updateMappings(autoMatchColumnMappings(sourceColumns, targetColumns, mappings));
  };

  const handleClearUnmapped = () => {
    updateMappings(clearUnmappedColumnMappings(mappings));
  };

  const targetOptions = (current: string) => {
    const cols = table.targetColumns ?? [];
    const options = cols.map((col) => ({ value: col, label: col }));
    if (current && !cols.includes(current)) {
      options.unshift({ value: current, label: current });
    }
    if (!options.some((o) => o.value === '')) {
      options.unshift({ value: '', label: t('transfer.mapping.pickTargetColumn') });
    }
    return options;
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3" data-testid="data-transfer-column-editor">
      <div className="flex flex-wrap items-end gap-3">
        <label className="flex min-w-[12rem] flex-1 flex-col gap-1 text-sm">
          <span className="text-fg-muted">{t('transfer.mapping.targetTable')}</span>
          <input
            type="text"
            className="rounded border border-border bg-bg px-2 py-1 font-mono text-sm"
            value={table.targetTable}
            data-testid="data-transfer-target-table-input"
            onChange={(e) => onChange({ targetTable: e.target.value })}
            onBlur={() => onTargetTableBlur?.()}
          />
        </label>
        {showCreateNewToggle && (
          <label className="flex items-center gap-2 pb-1 text-sm">
            <input
              type="checkbox"
              checked={table.createNew}
              data-testid="data-transfer-create-new-toggle"
              onChange={(e) => {
                const createNew = e.target.checked;
                onChange({
                  createNew,
                  targetTable:
                    createNew && !table.targetTable ? table.sourceTable : table.targetTable,
                });
              }}
            />
            {t('transfer.mapping.createNew')}
          </label>
        )}
      </div>

      <div className="flex flex-wrap gap-2">
        <Button
          variant="ghost"
          size="sm"
          data-testid="data-transfer-auto-match"
          onClick={handleAutoMatch}
        >
          {t('transfer.mapping.autoMatch')}
        </Button>
        <Button
          variant="ghost"
          size="sm"
          data-testid="data-transfer-clear-unmapped"
          onClick={handleClearUnmapped}
        >
          {t('transfer.mapping.clearUnmapped')}
        </Button>
      </div>

      {unmappedTargets.length > 0 && (
        <p className="text-xs text-warning" data-testid="data-transfer-unmapped-target-warning">
          {t('transfer.mapping.unmappedTargetWarning', {
            columns: unmappedTargets.join(', '),
          })}
        </p>
      )}

      <div className="min-h-0 flex-1 overflow-auto rounded-lg border border-border bg-surface">
        <div className="sticky top-0 flex items-center gap-3 border-b border-border bg-surface-alt px-3 py-1.5 text-[11px] font-semibold uppercase tracking-wider text-fg-muted">
          <div className="min-w-0 flex-1">{t('transfer.mapping.sourceColumn')}</div>
          {showTargetType && (
            <div className="w-28 shrink-0">{t('transfer.mapping.sourceType')}</div>
          )}
          <div className="min-w-0 flex-1">{t('transfer.mapping.targetColumn')}</div>
          {showTargetType && (
            <div className="w-36 shrink-0">{t('transfer.mapping.targetType')}</div>
          )}
          <div className="w-16 shrink-0 text-center">{t('transfer.mapping.skip')}</div>
        </div>
        {mappings.map((row) => (
          <ColumnMappingRow
            key={row.sourceColumn}
            row={row}
            createNew={table.createNew}
            showTargetType={showTargetType}
            sourceType={table.sourceColumnTypes?.[row.sourceColumn]}
            targetOptions={targetOptions(row.targetColumn)}
            onChange={(patch) => {
              updateMappings(
                mappings.map((m) => (m.sourceColumn === row.sourceColumn ? { ...m, ...patch } : m)),
              );
            }}
          />
        ))}
      </div>
    </div>
  );
}

function ColumnMappingRow({
  row,
  createNew,
  showTargetType,
  sourceType,
  targetOptions,
  onChange,
}: {
  row: TransferColumnMapping;
  createNew: boolean;
  showTargetType: boolean;
  sourceType?: string;
  targetOptions: { value: string; label: string }[];
  onChange: (patch: Partial<TransferColumnMapping>) => void;
}) {
  const unmapped = !row.skip && !row.targetColumn.trim();

  return (
    <div
      data-testid="data-transfer-column-row"
      className={cn(
        'flex items-center gap-3 border-t border-border px-3 py-1.5 text-sm',
        unmapped && 'bg-amber-500/5',
      )}
    >
      <div className="min-w-0 flex-1 truncate font-mono text-xs">{row.sourceColumn}</div>
      {showTargetType && (
        <div
          className="w-28 shrink-0 truncate font-mono text-[11px] text-fg-muted"
          title={sourceType}
        >
          {sourceType ?? '—'}
        </div>
      )}
      <div className="min-w-0 flex-1">
        {createNew ? (
          <input
            type="text"
            className="w-full rounded border border-border bg-bg px-2 py-1 font-mono text-xs"
            value={row.targetColumn}
            data-testid={`data-transfer-target-col-${row.sourceColumn}`}
            onChange={(e) =>
              onChange({ targetColumn: e.target.value, skip: !e.target.value.trim() })
            }
          />
        ) : (
          <div data-testid={`data-transfer-target-select-${row.sourceColumn}`}>
            <Select
              value={row.targetColumn}
              options={targetOptions}
              onChange={(value) => onChange({ targetColumn: value, skip: !value.trim() })}
              className="text-xs"
            />
          </div>
        )}
      </div>
      {showTargetType && (
        <div className="w-36 shrink-0">
          <input
            type="text"
            className="w-full rounded border border-border bg-bg px-2 py-1 font-mono text-[11px]"
            placeholder="VARCHAR(255)"
            value={row.targetNativeType ?? ''}
            data-testid={`data-transfer-target-type-${row.sourceColumn}`}
            onChange={(e) =>
              onChange({ targetNativeType: e.target.value.trim() ? e.target.value : undefined })
            }
          />
        </div>
      )}
      <div className="flex w-16 shrink-0 justify-center">
        <input
          type="checkbox"
          checked={row.skip ?? false}
          data-testid={`data-transfer-skip-${row.sourceColumn}`}
          onChange={(e) => onChange({ skip: e.target.checked })}
        />
      </div>
    </div>
  );
}
