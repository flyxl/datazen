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
import { onExportProgress } from '../../commands/file';
import { supportsFullTableExport, type DataExportCapability } from '../../lib/exportCapability';

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
  /**
   * Driver data-export capability. Batch export pulls entire tables, so it is
   * only allowed when this is `full_table`. Defaults to `full_table`. When not
   * `full_table`, the export button is disabled regardless of `onExport`.
   */
  dataExportCapability?: DataExportCapability;
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
  dataExportCapability = 'full_table',
}: BatchExportDialogProps) {
  const { t } = useI18n();
  const batchExportLocked = !supportsFullTableExport(dataExportCapability);
  const [selected, setSelected] = useState<Set<string>>(() => new Set(initialSelected ?? []));
  const [mode, setMode] = useState<BatchExportMode>('data_and_structure');
  const [dataFormat, setDataFormat] = useState<BatchExportDataFormat>('csv');
  const [outputMode, setOutputMode] = useState<BatchExportOutputMode>('zip');
  const [status, setStatus] = useState<'form' | 'exporting' | 'success'>('form');
  const [progress, setProgress] = useState<{
    current: number;
    total: number;
    tableName?: string;
    rowsWritten?: number;
  } | null>(null);
  const [error, setError] = useState<string | null>(null);

  const exporting = status === 'exporting';

  useEffect(() => {
    if (!open) return;
    setSelected(new Set(initialSelected ?? []));
    setMode('data_and_structure');
    setDataFormat('csv');
    setOutputMode('zip');
    setStatus('form');
    setProgress(null);
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
    if (batchExportLocked) return;
    setError(null);
    if (selected.size === 0) {
      setError(t('batchExport.noTablesSelected'));
      return;
    }

    const selectedTables = tables.filter((name) => selected.has(name));
    setStatus('exporting');
    setProgress({ current: 0, total: selectedTables.length });
    // Subscribe to Rust-side writer progress (row counts streamed to disk).
    let unlisten: (() => void) | undefined;
    try {
      const done = await onExportProgress((e) => {
        const idx = selectedTables.indexOf(e.table);
        setProgress({
          current: idx >= 0 ? idx + 1 : 1,
          total: selectedTables.length,
          tableName: e.table,
          rowsWritten: e.rowsWritten,
        });
      });
      unlisten = done;
    } catch {
      // Progress subscription is best-effort; proceed without live row counts.
    }
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
          loadTableExportData,
          onProgress: ({ tableName }) => {
            const idx = selectedTables.indexOf(tableName);
            setProgress((prev) => ({
              current: idx >= 0 ? idx + 1 : 1,
              total: selectedTables.length,
              tableName,
              rowsWritten: prev?.tableName === tableName ? prev.rowsWritten : undefined,
            }));
          },
        });
        if (result.status === 'cancelled') {
          // User dismissed the native save dialog; return to the form.
          setStatus('form');
          setProgress(null);
          return;
        }
      }
      setStatus('success');
      setProgress(null);
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e);
      setStatus('form');
      setError(
        message === 'no_tables_selected'
          ? t('batchExport.noTablesSelected')
          : `${t('batchExport.failed')}: ${message}`,
      );
    } finally {
      unlisten?.();
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
    batchExportLocked,
  ]);

  return (
    <Dialog
      open={open}
      title={t('batchExport.title')}
      // While exporting we don't allow closing (Rust runs the save dialog and
      // the whole stream in one native call).
      onClose={exporting ? () => undefined : onClose}
      className="max-w-lg"
      footer={
        status === 'success' ? (
          <Button variant="primary" onClick={onClose}>
            {t('common.close')}
          </Button>
        ) : (
          <>
            <Button variant="secondary" onClick={onClose} disabled={exporting}>
              {t('common.cancel')}
            </Button>
            <Button
              variant="primary"
              onClick={() => void handleExport()}
              disabled={exporting || batchExportLocked || selected.size === 0}
            >
              {exporting ? t('batchExport.exporting') : t('batchExport.export')}
            </Button>
          </>
        )
      }
    >
      {status === 'success' ? (
        <div className="flex flex-col items-center gap-2 py-6 text-center">
          <span className="flex h-12 w-12 items-center justify-center rounded-full bg-green-500/15 text-green-400">
            <svg
              viewBox="0 0 24 24"
              className="h-6 w-6"
              fill="none"
              stroke="currentColor"
              strokeWidth="2.5"
            >
              <path d="M20 6 9 17l-5-5" strokeLinecap="round" strokeLinejoin="round" />
            </svg>
          </span>
          <span className="text-sm font-medium text-fg">{t('batchExport.success')}</span>
        </div>
      ) : status === 'exporting' ? (
        <div className="flex flex-col items-center gap-3 py-6 text-center">
          <span className="h-5 w-5 animate-spin rounded-full border-2 border-accent border-t-transparent" />
          <span className="text-sm text-fg">
            {progress && progress.total > 0
              ? `${t('batchExport.exporting')} ${progress.current}/${progress.total}${
                  progress.tableName ? ` · ${progress.tableName}` : ''
                }`
              : t('batchExport.exporting')}
          </span>
          {progress && progress.total > 0 && (
            <div className="h-1.5 w-56 overflow-hidden rounded-full bg-surface-raised">
              <div
                className="h-full bg-accent transition-all duration-200"
                style={{ width: `${(progress.current / progress.total) * 100}%` }}
              />
            </div>
          )}
        </div>
      ) : (
        <div className="space-y-4">
          {batchExportLocked && (
            <div className="rounded-md border border-amber-500/20 bg-amber-500/10 px-3 py-2 text-xs text-amber-500">
              {t('batchExport.disabledByDriver')}
            </div>
          )}
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
                    />
                    <span className="text-xs text-fg-secondary">{name}</span>
                  </label>
                ))
              )}
            </div>
          </div>

          {/* Mode */}
          <fieldset className="space-y-1.5">
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
              />
            </div>
          )}

          {/* Output */}
          <fieldset className="space-y-1.5">
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
      )}
    </Dialog>
  );
}
