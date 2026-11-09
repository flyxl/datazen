import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Dialog } from '../../components/ui/Dialog';
import { CopyableError } from '../../components/ui/CopyableError';
import { Button } from '../../components/ui/Button';
import { Select } from '../../components/ui/Select';
import { ExportSuccessPanel } from '../../components/export/ExportSuccessPanel';
import { ExportColumnPicker } from '../../components/export/ExportColumnPicker';
import { generateExport } from '../../lib/exportData';
import type { ExportFormat, ExportScope } from '../../lib/exportData';
import {
  buildExportFormatOptions,
  buildExportScopeOptions,
  EXPORT_FORMAT_OPTIONS,
  saveExportResultWithDialog,
} from '../../lib/exportDialogShared';
import {
  isStreamableExportFormat,
  streamTableExportToSaveDialog,
  type TableExportRunHandle,
} from '../../lib/exportStream';
import { supportsFullTableExport, type DataExportCapability } from '../../lib/exportCapability';
import type { ColumnSchema } from '../../types';
import { useI18n } from '../../hooks/useI18n';
import { getCachedTableSchema } from '../../lib/schemaCache';
import { useSchemaStore } from '../../stores/schemaStore';

interface ExportDialogProps {
  open: boolean;
  onClose: () => void;
  tableName: string;
  columns: ColumnSchema[];
  rows: Record<string, unknown>[];
  selectedRows: Set<number>;
  databaseType?: string;
  dbSessionId?: string;
  /** Panel-bound database — the schema prefetch for entire-table export runs against it. */
  database?: string;
  totalRows?: number;
  /** Prefer entire-table when opened from the schema tree (no page loaded). */
  defaultScope?: ExportScope;
  /**
   * Driver data-export capability. Defaults to `full_table`.
   * - `none`: export disabled entirely.
   * - `loaded_only`: only already-loaded rows may be exported (no entire-table).
   */
  dataExportCapability?: DataExportCapability;
}

export function ExportDialog({
  open,
  onClose,
  tableName,
  columns,
  rows,
  selectedRows,
  databaseType,
  dbSessionId,
  database,
  totalRows,
  defaultScope,
  dataExportCapability = 'full_table',
}: ExportDialogProps) {
  const { t } = useI18n();
  const tableSchema = useSchemaStore((s) =>
    dbSessionId ? s.schemaOfRelation(tableName, dbSessionId) : null,
  );
  const exportLocked = dataExportCapability === 'none';
  const allowEntire =
    !exportLocked && supportsFullTableExport(dataExportCapability) && Boolean(dbSessionId);
  const [format, setFormat] = useState<ExportFormat>('csv');
  const [scope, setScope] = useState<ExportScope>(
    defaultScope ?? (allowEntire && rows.length === 0 ? 'entire_table' : 'current_page'),
  );
  const [loadedColumns, setLoadedColumns] = useState<ColumnSchema[]>(columns);
  const [selectedCols, setSelectedCols] = useState<Set<string>>(
    () => new Set(columns.map((c) => c.name)),
  );
  const [status, setStatus] = useState<'form' | 'exporting' | 'success' | 'aborted'>('form');
  const [exporting, setExporting] = useState(false);
  /** The cancel was asked for; the run is winding down (DBR-06). */
  const [cancelling, setCancelling] = useState(false);
  /** Rows written so far by a streaming export (DBR-06 行数进度). */
  const [rowsWritten, setRowsWritten] = useState(0);
  /** Whether the aborted run's partial file was deleted (reported honestly). */
  const [partialDeleted, setPartialDeleted] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const runHandleRef = useRef<TableExportRunHandle | null>(null);

  const isStreamingScope = scope === 'entire_table';

  useEffect(() => {
    setLoadedColumns(columns);
    setSelectedCols(new Set(columns.map((c) => c.name)));
    if (!open) return;
    setStatus('form');
    setError(null);
    setCancelling(false);
    setRowsWritten(0);
    if (!dbSessionId || !database) return;
    let cancelled = false;
    void getCachedTableSchema(dbSessionId, tableName, database, tableSchema)
      .then((schema) => {
        if (cancelled || schema.columns.length === 0) return;
        setLoadedColumns(schema.columns);
        // The schema arrives after open: the column default is still "every
        // column" (DBR-06 恢复默认) — the fuller list, freshly selected.
        setSelectedCols(new Set(schema.columns.map((c) => c.name)));
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [open, dbSessionId, database, tableName, columns, tableSchema]);

  const selectedColumnNames = useMemo(
    () => loadedColumns.map((c) => c.name).filter((n) => selectedCols.has(n)),
    [loadedColumns, selectedCols],
  );

  // DBR-06 格式统一: the same list, gating and reasons as the grid dialog;
  // SQL UPDATE needs a key column instead of silently using column 0.
  const hasKeyColumn = loadedColumns.some((c) => c.isPrimaryKey);
  const formatOptions = useMemo(
    () => buildExportFormatOptions({ streamingScope: isStreamingScope, hasKeyColumn }),
    [isStreamingScope, hasKeyColumn],
  );

  // An unavailable format (scope became streaming, or no key column) falls
  // back to CSV instead of keeping a disabled selection (DBR-06) — the
  // disabled option stays visible with its reason.
  useEffect(() => {
    if (formatOptions.find((o) => o.value === format)?.disabled) setFormat('csv');
  }, [formatOptions, format]);

  const rowCount = useMemo(() => {
    if (scope === 'entire_table') return totalRows ?? rows.length;
    if (scope === 'selected' && selectedRows.size > 0) return selectedRows.size;
    return rows.length;
  }, [scope, selectedRows, rows, totalRows]);

  const scopeOptions = useMemo(
    () =>
      buildExportScopeOptions(
        t,
        rows.length,
        selectedRows.size,
        allowEntire ? { totalRows } : false,
      ),
    [t, rows.length, selectedRows.size, allowEntire, totalRows],
  );

  const handleScopeChange = useCallback(
    (v: string) => {
      const next = v as ExportScope;
      setScope(next);
      setRowsWritten(0);
      if (next === 'entire_table' && !isStreamableExportFormat(format)) {
        setFormat('csv');
      }
    },
    [format],
  );

  const handleFormatChange = useCallback(
    (v: string) => {
      const next = v as ExportFormat;
      if (formatOptions.find((o) => o.value === next)?.disabled) return;
      setFormat(next);
    },
    [formatOptions],
  );

  const toggleColumn = useCallback((col: string) => {
    setSelectedCols((prev) => {
      const next = new Set(prev);
      if (next.has(col)) next.delete(col);
      else next.add(col);
      return next;
    });
  }, []);

  const resetColumnsToDefault = useCallback(() => {
    setSelectedCols(new Set(loadedColumns.map((c) => c.name)));
  }, [loadedColumns]);

  const handleCancelExport = useCallback(() => {
    if (cancelling) return;
    setCancelling(true);
    runHandleRef.current?.cancel();
  }, [cancelling]);

  const handleExport = useCallback(async () => {
    setError(null);
    setExporting(true);
    setStatus('exporting');
    setCancelling(false);
    setRowsWritten(0);
    runHandleRef.current = null;
    try {
      const colNames = selectedColumnNames;
      if (scope === 'entire_table') {
        if (!dbSessionId) {
          throw new Error('Missing connection');
        }
        if (!isStreamableExportFormat(format)) {
          throw new Error('xlsx');
        }
        const pkName = loadedColumns.find((c) => c.isPrimaryKey)?.name;
        const result = await streamTableExportToSaveDialog({
          dbSessionId,
          tableName,
          // DBR-06: only the picked columns, in display (schema) order.
          columns: colNames,
          format,
          databaseType,
          pkName,
          onProgress: (written) => setRowsWritten(written),
          onRunStart: (handle) => {
            runHandleRef.current = handle;
          },
        });
        if (result.status === 'cancelled') {
          setStatus('form');
          setExporting(false);
          return;
        }
        if (result.status === 'aborted') {
          setPartialDeleted(result.partialDeleted);
          setStatus('aborted');
          return;
        }
        setStatus('success');
        return;
      }

      const result = generateExport({
        tableName,
        columns: loadedColumns,
        rows,
        selectedRows,
        scope,
        selectedColumns: colNames,
        format,
        databaseType,
      });

      const saved = await saveExportResultWithDialog(result, tableName, format);
      if (!saved) {
        setStatus('form');
        setExporting(false);
        return;
      }
      setStatus('success');
    } catch (e) {
      setStatus('form');
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setExporting(false);
    }
  }, [
    loadedColumns,
    selectedColumnNames,
    tableName,
    rows,
    selectedRows,
    scope,
    format,
    databaseType,
    dbSessionId,
  ]);

  return (
    <Dialog
      open={open}
      title={t('common.exportData')}
      description={t('export.description', { table: tableName })}
      onClose={onClose}
      footer={
        status === 'success' || status === 'aborted' ? (
          <Button variant="primary" onClick={onClose}>
            {t('common.close')}
          </Button>
        ) : (
          <>
            {isStreamingScope && (
              <Button
                variant="secondary"
                onClick={handleCancelExport}
                disabled={!exporting || cancelling}
              >
                {cancelling ? t('export.cancelling') : t('export.cancelExport')}
              </Button>
            )}
            <Button variant="secondary" onClick={onClose} disabled={exporting}>
              {t('common.cancel')}
            </Button>
            <Button
              variant="primary"
              onClick={() => void handleExport()}
              disabled={
                exporting || exportLocked || (loadedColumns.length > 0 && selectedCols.size === 0)
              }
            >
              {exporting ? t('export.exporting') : t('export.export')}
            </Button>
          </>
        )
      }
    >
      {status === 'success' ? (
        <ExportSuccessPanel message={t('export.success')} />
      ) : status === 'aborted' ? (
        // DBR-06: the cancel reports what happened to the partial file, as
        // the backend answered — deleted or kept are two different messages.
        <div
          data-testid="export-aborted-panel"
          className="rounded-md border border-edge bg-surface px-3 py-2 text-xs text-fg-secondary"
        >
          {partialDeleted ? t('export.cancelledPartialDeleted') : t('export.cancelledPartialKept')}
        </div>
      ) : (
        <div className="space-y-4">
          {exportLocked && (
            <div className="rounded-md border border-amber-500/20 bg-amber-500/10 px-3 py-2 text-xs text-amber-500">
              {t('export.disabledByDriver')}
            </div>
          )}
          <div>
            <label className="mb-1 block text-xs font-medium text-fg-secondary">
              {t('export.format')}
            </label>
            <Select value={format} options={formatOptions} onChange={handleFormatChange} />
            {/* 禁用并解释原因（DBR-06）：an unavailable format stays listed and
             * greyed out, and its reason is on screen the whole time the
             * range makes it unavailable — not only while it is selected. */}
            {isStreamingScope && (
              <p data-testid="export-format-disabled-reason" className="mt-1 text-xs text-fg-muted">
                {t('export.formatDisabledStream')}
              </p>
            )}
            {!hasKeyColumn && (
              <p data-testid="export-format-disabled-reason" className="mt-1 text-xs text-fg-muted">
                {t('export.formatDisabledNoKey')}
              </p>
            )}
          </div>

          <div>
            <label className="mb-1 block text-xs font-medium text-fg-secondary">
              {t('export.range')}
            </label>
            <Select value={scope} options={scopeOptions} onChange={handleScopeChange} />
          </div>

          <ExportColumnPicker
            columns={loadedColumns.map((c) => ({
              name: c.name,
              type: c.dataType,
              isPrimaryKey: c.isPrimaryKey,
            }))}
            selected={selectedCols}
            onToggle={toggleColumn}
            onSelectAll={() => setSelectedCols(new Set(loadedColumns.map((c) => c.name)))}
            onClearAll={() => setSelectedCols(new Set())}
            onResetDefault={resetColumnsToDefault}
          />

          <div className="rounded-md border border-edge bg-surface px-3 py-2 text-xs text-fg-muted">
            {t('export.willExport', { rows: rowCount, cols: selectedCols.size })},{' '}
            {t('export.formatAs')}{' '}
            <span className="font-medium text-fg-secondary">
              {EXPORT_FORMAT_OPTIONS.find((o) => o.value === format)?.label}
            </span>
          </div>

          {exporting && isStreamingScope && rowsWritten > 0 && (
            <div
              data-testid="export-progress"
              className="rounded-md border border-edge bg-surface px-3 py-2 text-xs text-fg-secondary"
            >
              {t('export.progressRows', { rows: rowsWritten })}
            </div>
          )}

          {error && (
            <CopyableError
              message={error}
              className="rounded-md border border-red-500/20 bg-red-500/10 px-3 py-2 text-xs text-red-400"
            />
          )}
        </div>
      )}
    </Dialog>
  );
}
