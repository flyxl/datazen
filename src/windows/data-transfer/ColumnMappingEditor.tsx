import { Button } from '../../components/ui/Button';
import { Checkbox } from '../../components/ui/Checkbox';
import { Input } from '../../components/ui/Input';
import { Select } from '../../components/ui/Select';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';
import { SourceFilterEditor } from './SourceFilterEditor';
import type {
  TransferColumnMapping,
  TransferRecordset,
  TransferRecordsetBound,
  TransferRecordsetTupleBound,
  TransferTableResult,
} from '../../commands/transfer';
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
          <Input
            value={table.targetTable}
            data-testid="data-transfer-target-table-input"
            onChange={(e) => onChange({ targetTable: e.target.value })}
            onBlur={() => onTargetTableBlur?.()}
          />
        </label>
        {showCreateNewToggle && (
          <label className="flex items-center gap-2 pb-1 text-sm">
            <Checkbox
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

      <div className="min-h-0 flex-1 overflow-auto rounded-lg border border-edge bg-surface">
        <div className="sticky top-0 flex items-center gap-3 border-b border-edge bg-surface-alt px-3 py-1.5 text-[11px] font-semibold uppercase tracking-wider text-fg-muted">
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
      <RecordsetEditor table={table} onChange={(recordset) => onChange({ recordset })} />
      <SourceFilterEditor
        columns={table.sourceColumns ?? mappings.map((mapping) => mapping.sourceColumn)}
        filter={table.sourceFilter}
        onChange={(sourceFilter) => onChange({ sourceFilter })}
      />
    </div>
  );
}

function RecordsetEditor({
  table,
  onChange,
}: {
  table: TransferTableResult;
  onChange: (recordset: TransferRecordset | undefined) => void;
}) {
  const { t } = useI18n();
  const recordset = table.recordset;
  const columns = table.sourceColumns ?? [];
  const primaryKeys = table.sourcePrimaryKeys ?? [];
  const defaultOrder = primaryKeys.length === 1 ? primaryKeys[0] : undefined;
  const tupleRange = recordset?.tupleRange;
  const orderBy = recordset?.orderBy ?? defaultOrder ?? '';
  const orderOptions = [
    { value: '', label: t('transfer.mapping.recordsetOrderRequired') },
    ...columns.map((column) => ({
      value: column,
      label: table.sourceColumnTypes?.[column]
        ? `${column} (${table.sourceColumnTypes[column]})`
        : column,
    })),
  ];

  const update = (patch: Partial<TransferRecordset>) => {
    onChange({
      ...(recordset ?? {}),
      ...patch,
    });
  };

  const updateBound = (name: 'start' | 'end', patch: Partial<TransferRecordsetBound>) => {
    const current = recordset?.[name];
    const next = { ...(current ?? { value: '', inclusive: true }), ...patch };
    if (!next.value.trim()) {
      onChange({
        ...(recordset ?? {}),
        [name]: undefined,
      });
      return;
    }
    update({ [name]: next });
  };

  const updateTupleBound = (name: 'start' | 'end', patch: Partial<TransferRecordsetTupleBound>) => {
    const current = tupleRange?.[name];
    const next = {
      ...(current ?? { values: primaryKeys.map(() => ''), inclusive: true }),
      ...patch,
    };
    if (next.values.every((value) => !value.trim())) {
      onChange({
        ...(recordset ?? {}),
        tupleRange: {
          columns: primaryKeys,
          ...(tupleRange?.start && name !== 'start' ? { start: tupleRange.start } : {}),
          ...(tupleRange?.end && name !== 'end' ? { end: tupleRange.end } : {}),
        },
      });
      return;
    }
    onChange({
      ...(recordset ?? {}),
      orderBy: undefined,
      start: undefined,
      end: undefined,
      tupleRange: {
        columns: primaryKeys,
        ...(name === 'start'
          ? { start: next }
          : tupleRange?.start
            ? { start: tupleRange.start }
            : {}),
        ...(name === 'end' ? { end: next } : tupleRange?.end ? { end: tupleRange.end } : {}),
      },
    });
  };

  return (
    <div
      className="space-y-2 rounded-lg border border-edge bg-surface-alt p-3"
      data-testid="data-transfer-recordset"
    >
      <div className="flex items-start justify-between gap-2">
        <div>
          <div className="text-sm font-medium">{t('transfer.mapping.recordset')}</div>
          <div className="text-xs text-fg-muted">{t('transfer.mapping.recordsetHint')}</div>
        </div>
        <Checkbox
          checked={Boolean(recordset)}
          data-testid="data-transfer-recordset-enable"
          onChange={(event) => {
            onChange(
              event.target.checked
                ? primaryKeys.length > 1
                  ? { tupleRange: { columns: primaryKeys } }
                  : { orderBy: defaultOrder }
                : undefined,
            );
          }}
        />
      </div>
      {!recordset ? (
        <p className="text-xs text-fg-muted">{t('transfer.mapping.noRecordset')}</p>
      ) : (
        <div className="space-y-2" data-testid="data-transfer-recordset-editor">
          {tupleRange ? (
            <div className="space-y-2" data-testid="data-transfer-recordset-tuple-editor">
              <div className="text-xs text-fg-muted">
                {t('transfer.mapping.recordsetOrder')}: {primaryKeys.join(' → ')}
              </div>
              <p
                className="text-xs text-warning"
                data-testid="data-transfer-recordset-tuple-collation-hint"
              >
                {t('transfer.mapping.recordsetTextCollationHint')}
              </p>
              <div className="grid gap-2 sm:grid-cols-2">
                {(['start', 'end'] as const).map((name) => {
                  const bound = tupleRange[name];
                  return (
                    <div key={name} className="space-y-1 rounded border border-edge p-2 text-xs">
                      <span className="text-fg-muted">
                        {name === 'start'
                          ? t('transfer.mapping.recordsetStart')
                          : t('transfer.mapping.recordsetEnd')}
                      </span>
                      {primaryKeys.map((column, index) => (
                        <label key={column} className="flex items-center gap-2">
                          <span className="w-24 shrink-0 truncate font-mono">{column}</span>
                          <Input
                            value={bound?.values[index] ?? ''}
                            placeholder={t('transfer.mapping.recordsetUnbounded')}
                            data-testid={`data-transfer-recordset-tuple-${name}-${index}`}
                            onChange={(event) => {
                              const values = bound?.values.slice() ?? primaryKeys.map(() => '');
                              values[index] = event.target.value;
                              updateTupleBound(name, { values });
                            }}
                            className="h-7 min-w-0 flex-1 rounded border border-edge bg-surface px-2 text-xs"
                          />
                        </label>
                      ))}
                      {bound && (
                        <label className="flex items-center gap-1 text-fg-muted">
                          <Checkbox
                            checked={bound.inclusive ?? true}
                            data-testid={`data-transfer-recordset-tuple-${name}-inclusive`}
                            onChange={(event) =>
                              updateTupleBound(name, { inclusive: event.target.checked })
                            }
                          />
                          {t('transfer.mapping.recordsetInclusive')}
                        </label>
                      )}
                    </div>
                  );
                })}
              </div>
            </div>
          ) : (
            <>
              <label className="flex flex-wrap items-center gap-2 text-xs">
                <span className="text-fg-muted">{t('transfer.mapping.recordsetOrder')}</span>
                <Select
                  value={orderBy}
                  options={orderOptions}
                  onChange={(value) => update({ orderBy: value || undefined })}
                  className="!h-7 min-w-48 !text-xs"
                  triggerDataAttrs={{ 'data-testid': 'data-transfer-recordset-order' }}
                />
              </label>
              {!orderBy && (
                <p
                  className="text-xs text-warning"
                  data-testid="data-transfer-recordset-order-error"
                >
                  {t('transfer.mapping.recordsetOrderRequired')}
                </p>
              )}
              <div className="grid gap-2 sm:grid-cols-2">
                {(['start', 'end'] as const).map((name) => {
                  const bound = recordset[name];
                  return (
                    <label key={name} className="flex flex-col gap-1 text-xs">
                      <span className="text-fg-muted">
                        {name === 'start'
                          ? t('transfer.mapping.recordsetStart')
                          : t('transfer.mapping.recordsetEnd')}
                      </span>
                      <div className="flex items-center gap-2">
                        <Input
                          value={bound?.value ?? ''}
                          placeholder={t('transfer.mapping.recordsetUnbounded')}
                          data-testid={`data-transfer-recordset-${name}`}
                          onChange={(event) => updateBound(name, { value: event.target.value })}
                          className="h-7 min-w-0 flex-1 rounded border border-edge bg-surface px-2 text-xs"
                        />
                        {bound && (
                          <label className="flex shrink-0 items-center gap-1 text-fg-muted">
                            <Checkbox
                              checked={bound.inclusive ?? true}
                              data-testid={`data-transfer-recordset-${name}-inclusive`}
                              onChange={(event) =>
                                updateBound(name, { inclusive: event.target.checked })
                              }
                            />
                            {t('transfer.mapping.recordsetInclusive')}
                          </label>
                        )}
                      </div>
                    </label>
                  );
                })}
              </div>
            </>
          )}
          <label className="flex items-center gap-2 text-xs">
            <span className="text-fg-muted">{t('transfer.mapping.recordsetLimit')}</span>
            <Input
              type="number"
              min={1}
              step={1}
              value={recordset.limit ?? ''}
              data-testid="data-transfer-recordset-limit"
              onChange={(event) => {
                const value = event.target.value.trim();
                update({ limit: value ? Number(value) : undefined });
              }}
              className="h-7 w-32 rounded border border-edge bg-surface px-2 text-xs"
            />
          </label>
        </div>
      )}
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
        'flex items-center gap-3 border-t border-edge px-3 py-1.5 text-sm',
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
          <Input
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
          <Input
            className="text-[11px]"
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
        <Checkbox
          checked={row.skip ?? false}
          data-testid={`data-transfer-skip-${row.sourceColumn}`}
          onChange={(e) => onChange({ skip: e.target.checked })}
        />
      </div>
    </div>
  );
}
