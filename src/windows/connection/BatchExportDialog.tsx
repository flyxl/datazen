import { useCallback, useEffect, useMemo, useState } from 'react';
import { Dialog } from '../../components/ui/Dialog';
import { Button } from '../../components/ui/Button';
import { Select } from '../../components/ui/Select';
import { useI18n } from '../../hooks/useI18n';
import type {
  BatchExportDataFormat,
  BatchExportMode,
  BatchExportTableInput,
} from '../../lib/batchExport';
import { runBatchExportJob, type BatchExportOutputMode } from '../../lib/batchExportJob';
import { queryCommands } from '../../commands/query';

export interface BatchExportDialogProps {
  open: boolean;
  onClose: () => void;
  connectionId: string;
  databaseType?: string;
  database?: string;
  /** available table names (from schema store) */
  tables: string[];
  /** optional pre-selected table names */
  initialSelected?: string[];
  /** Required loader used by the export job (mock in tests; F3 wires real fetch). */
  loadTableExportData: (tableName: string) => Promise<BatchExportTableInput>;
  /**
   * Optional full override of the export action (e.g. custom wiring).
   * When omitted, uses runBatchExportJob + loadTableExportData.
   */
  onExport?: (opts: {
    selectedTables: string[];
    mode: BatchExportMode;
    dataFormat: BatchExportDataFormat;
    outputMode: BatchExportOutputMode;
  }) => Promise<void>;
}

const DATA_FORMAT_OPTIONS: { value: BatchExportDataFormat; label: string }[] = [
  { value: 'csv', label: 'CSV' },
  { value: 'json', label: 'JSON' },
  { value: 'sql_insert', label: 'SQL INSERT' },
];

export function BatchExportDialog({
  open,
  onClose,
  connectionId,
  databaseType,
  database: _database,
  tables,
  initialSelected,
  loadTableExportData,
  onExport,
}: BatchExportDialogProps) {
  const { t } = useI18n();
  const [selected, setSelected] = useState<Set<string>>(() => new Set(initialSelected ?? []));
  const [mode, setMode] = useState<BatchExportMode>('data_and_structure');
  const [dataFormat, setDataFormat] = useState<BatchExportDataFormat>('csv');
  const [outputMode, setOutputMode] = useState<BatchExportOutputMode>('zip');
  const [exporting, setExporting] = useState(false);
  const [progressLabel, setProgressLabel] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    setSelected(new Set(initialSelected ?? []));
    setMode('data_and_structure');
    setDataFormat('csv');
    setOutputMode('zip');
    setExporting(false);
    setProgressLabel(null);
    setError(null);
  }, [open, initialSelected]);

  const allSelected = tables.length > 0 && selected.size === tables.length;

  const toggleTable = useCallback((name: string) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(name)) next.delete(name);
      else next.add(name);
      return next;
    });
  }, []);

  const selectAll = useCallback(() => {
    setSelected(new Set(tables));
  }, [tables]);

  const clearAll = useCallback(() => {
    setSelected(new Set());
  }, []);

  const modeOptions = useMemo(
    () => [
      { value: 'structure_only' as const, label: t('batchExport.modeStructureOnly') },
      { value: 'data_only' as const, label: t('batchExport.modeDataOnly') },
      { value: 'data_and_structure' as const, label: t('batchExport.modeDataAndStructure') },
    ],
    [t],
  );

  const formatDisabled = mode === 'structure_only';

  const handleExport = useCallback(async () => {
    setError(null);
    if (selected.size === 0) {
      setError(t('batchExport.noTablesSelected'));
      return;
    }

    const selectedTables = tables.filter((name) => selected.has(name));
    setExporting(true);
    setProgressLabel(t('batchExport.exporting'));
    try {
      if (onExport) {
        await onExport({ selectedTables, mode, dataFormat, outputMode });
      } else {
        const result = await runBatchExportJob({
          tableNames: selectedTables,
          mode,
          dataFormat,
          outputMode,
          databaseType,
          connectionId,
          streamQuery: queryCommands.executeQueryStream,
          loadTableExportData,
          onProgress: ({ current, total, tableName }) => {
            setProgressLabel(`${t('batchExport.exporting')} (${current}/${total}: ${tableName})`);
          },
        });
        if (result.status === 'cancelled') {
          return;
        }
      }
      onClose();
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e);
      setError(
        message === 'no_tables_selected'
          ? t('batchExport.noTablesSelected')
          : `${t('batchExport.failed')}: ${message}`,
      );
    } finally {
      setExporting(false);
      setProgressLabel(null);
    }
  }, [
    selected,
    tables,
    t,
    onExport,
    mode,
    dataFormat,
    outputMode,
    databaseType,
    loadTableExportData,
    connectionId,
    onClose,
  ]);

  return (
    <Dialog
      open={open}
      title={t('batchExport.title')}
      onClose={onClose}
      className="max-w-lg"
      footer={
        <>
          <Button variant="secondary" onClick={onClose} disabled={exporting}>
            {t('common.cancel')}
          </Button>
          <Button
            variant="primary"
            onClick={() => void handleExport()}
            disabled={exporting || selected.size === 0}
          >
            {exporting ? (progressLabel ?? t('batchExport.exporting')) : t('batchExport.export')}
          </Button>
        </>
      }
    >
      <div className="space-y-4">
        {/* Table selection */}
        <div>
          <div className="mb-1 flex items-center justify-between">
            <label className="text-xs font-medium text-fg-secondary">
              {t('batchExport.selectTables')}
            </label>
            <div className="flex gap-2">
              <button
                type="button"
                className="text-xs text-accent hover:underline disabled:opacity-50"
                onClick={selectAll}
                disabled={tables.length === 0 || allSelected}
              >
                {t('batchExport.selectAll')}
              </button>
              <button
                type="button"
                className="text-xs text-accent hover:underline disabled:opacity-50"
                onClick={clearAll}
                disabled={selected.size === 0}
              >
                {t('batchExport.clearAll')}
              </button>
            </div>
          </div>
          <div className="max-h-40 overflow-y-auto rounded-md border border-edge bg-surface p-2">
            {tables.length === 0 ? (
              <div className="px-1 py-2 text-xs text-fg-muted">
                {t('batchExport.noTablesSelected')}
              </div>
            ) : (
              tables.map((name) => (
                <label
                  key={name}
                  className="flex cursor-pointer items-center gap-2 rounded px-1 py-0.5 hover:bg-surface-raised"
                >
                  <input
                    type="checkbox"
                    checked={selected.has(name)}
                    onChange={() => toggleTable(name)}
                    className="accent-accent"
                    disabled={exporting}
                  />
                  <span className="text-xs text-fg-secondary">{name}</span>
                </label>
              ))
            )}
          </div>
        </div>

        {/* Mode */}
        <fieldset className="space-y-1.5" disabled={exporting}>
          <legend className="mb-1 text-xs font-medium text-fg-secondary">
            {t('batchExport.mode')}
          </legend>
          {modeOptions.map((opt) => (
            <label key={opt.value} className="flex cursor-pointer items-center gap-2">
              <input
                type="radio"
                name="batch-export-mode"
                value={opt.value}
                checked={mode === opt.value}
                onChange={() => setMode(opt.value)}
                className="accent-accent"
              />
              <span className="text-xs text-fg-secondary">{opt.label}</span>
            </label>
          ))}
        </fieldset>

        {/* Data format */}
        {!formatDisabled && (
          <div>
            <label className="mb-1 block text-xs font-medium text-fg-secondary">
              {t('batchExport.format')}
            </label>
            <Select
              value={dataFormat}
              options={DATA_FORMAT_OPTIONS}
              onChange={(v) => setDataFormat(v as BatchExportDataFormat)}
              disabled={exporting}
            />
          </div>
        )}

        {/* Output */}
        <fieldset className="space-y-1.5" disabled={exporting}>
          <legend className="mb-1 text-xs font-medium text-fg-secondary">
            {t('batchExport.output')}
          </legend>
          <label className="flex cursor-pointer items-center gap-2">
            <input
              type="radio"
              name="batch-export-output"
              value="single"
              checked={outputMode === 'single'}
              onChange={() => setOutputMode('single')}
              className="accent-accent"
            />
            <span className="text-xs text-fg-secondary">{t('batchExport.outputSingle')}</span>
          </label>
          <label className="flex cursor-pointer items-center gap-2">
            <input
              type="radio"
              name="batch-export-output"
              value="zip"
              checked={outputMode === 'zip'}
              onChange={() => setOutputMode('zip')}
              className="accent-accent"
            />
            <span className="text-xs text-fg-secondary">{t('batchExport.outputZip')}</span>
          </label>
        </fieldset>

        {error && (
          <div className="rounded-md border border-red-500/20 bg-red-500/10 px-3 py-2 text-xs text-red-400">
            {error}
          </div>
        )}
      </div>
    </Dialog>
  );
}
