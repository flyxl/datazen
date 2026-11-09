import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Dialog } from '../../components/ui/Dialog';
import { CopyableError } from '../../components/ui/CopyableError';
import { Button } from '../../components/ui/Button';
import { Select } from '../../components/ui/Select';
import { ExportSuccessPanel } from '../../components/export/ExportSuccessPanel';
import type { ExportFormat, ExportScope } from '../../lib/exportData';
import {
  OBJECT_EXPORT_FORMAT_OPTIONS,
  buildExportScopeOptions,
  formatOptionsForScope,
} from '../../lib/exportDialogShared';
import { IS_DESKTOP_BUILD } from '../../platform/env';
import { exportSingleTableViaRust } from '../../lib/batchExportJob';
import { resolveDataFetcher } from '../../lib/dataFetcher';
import { supportsFullTableExport, type DataExportCapability } from '../../lib/exportCapability';
import type { ColumnSchema } from '../../types';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';
import { getCachedTableSchema } from '../../lib/schemaCache';
import { dataTypeTextClass } from '../../lib/dataTypeColors';
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
  const [exporting, setExporting] = useState(false);
  const [status, setStatus] = useState<'form' | 'exporting' | 'success' | 'aborted'>('form');
  /** The cancel was asked for; the run is winding down (DBR-06, restored). */
  const [cancelling, setCancelling] = useState(false);
  /** Whether the aborted run's partial file was deleted (reported honestly). */
  const [partialDeleted, setPartialDeleted] = useState(false);
  const [error, setError] = useState<string | null>(null);
  /** Cancel handle of the running SQL export; null on the provided path (no cancel there). */
  const cancelExportRef = useRef<(() => void) | null>(null);

  useEffect(() => {
    setLoadedColumns(columns);
    setSelectedCols(new Set(columns.map((c) => c.name)));
    if (!open) return;
    setStatus('form');
    setError(null);
    setCancelling(false);
    setPartialDeleted(false);
    cancelExportRef.current = null;
    if (!dbSessionId || !database) return;
    let cancelled = false;
    void getCachedTableSchema(dbSessionId, tableName, database, tableSchema)
      .then((schema) => {
        if (cancelled || schema.columns.length === 0) return;
        setLoadedColumns(schema.columns);
        setSelectedCols(new Set(schema.columns.map((c) => c.name)));
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [open, dbSessionId, database, tableName, columns, tableSchema]);

  const toggleColumn = useCallback((col: string) => {
    setSelectedCols((prev) => {
      const next = new Set(prev);
      if (next.has(col)) next.delete(col);
      else next.add(col);
      return next;
    });
  }, []);

  const toggleAll = useCallback(() => {
    setSelectedCols((prev) => {
      if (prev.size === loadedColumns.length) return new Set();
      return new Set(loadedColumns.map((c) => c.name));
    });
  }, [loadedColumns]);

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

  // Task 6 (plan §2.8): on web, the entire-table scope can only be CSV —
  // it goes through the web exporter, which has no SQL path — while the
  // provided scopes format in JS, so every format stays available there.
  // Desktop offers every format for every scope.
  const formatOptions = formatOptionsForScope(
    OBJECT_EXPORT_FORMAT_OPTIONS,
    scope,
    IS_DESKTOP_BUILD,
  );

  // A format picked for a provided scope must not survive a switch to the
  // web entire-table scope, where it could not be served (the Select would
  // point at a missing option). Desktop never resets: every format is served.
  useEffect(() => {
    if (!IS_DESKTOP_BUILD && scope === 'entire_table' && format !== 'csv') {
      setFormat('csv');
    }
  }, [scope, format]);

  const handleScopeChange = useCallback((v: string) => {
    setScope(v as ExportScope);
  }, []);

  const handleCancelExport = useCallback(() => {
    if (cancelling) return;
    setCancelling(true);
    cancelExportRef.current?.();
  }, [cancelling]);

  const handleExport = useCallback(async () => {
    setError(null);
    setExporting(true);
    setStatus('exporting');
    setCancelling(false);
    setPartialDeleted(false);
    cancelExportRef.current = null;
    try {
      const colNames = loadedColumns.map((c) => c.name).filter((n) => selectedCols.has(n));
      if (scope === 'entire_table') {
        if (!dbSessionId) {
          throw new Error('Missing connection');
        }
        const pkName = loadedColumns.find((c) => c.isPrimaryKey)?.name;
        // Item 3b: entire-table rows stream through the Rust batch
        // exporter (single-table request) — no frontend streaming path.
        // The run is cancellable mid-flight (DBR-06, restored): the
        // handle arrives before the export starts.
        const result = await exportSingleTableViaRust({
          dbSessionId,
          databaseType,
          database,
          tableName,
          columns: colNames,
          dataFormat: format,
          primaryKey: pkName,
          onCancelHandle: (cancel) => {
            cancelExportRef.current = cancel;
          },
        });
        if (result.status === 'cancelled') {
          setStatus('form');
          setExporting(false);
          return;
        }
        if (result.status === 'aborted') {
          setPartialDeleted(result.partialDeleted);
          setCancelling(false);
          setStatus('aborted');
          return;
        }
        setStatus('success');
        return;
      }

      // Task 4: current-page / selected rows are exported by
      // `ProvidedDataFetcher` (pure frontend: formatted in memory, saved
      // via the save dialog / Blob download — no Rust `provided` source
      // any more). No session is needed on this path, so the guard that
      // used to sit here is gone; entire-table above still requires one.
      const pkName = loadedColumns.find((c) => c.isPrimaryKey)?.name;
      const scopedRows =
        scope === 'selected' && selectedRows.size > 0
          ? rows.filter((_, i) => selectedRows.has(i))
          : rows;
      const result = await resolveDataFetcher({
        tableName,
        scope,
        columns: loadedColumns
          .filter((c) => selectedCols.has(c.name))
          .map((c) => ({ name: c.name, dataType: c.dataType, nullable: c.nullable })),
        rows: scopedRows.map((row) => colNames.map((col) => row[col])),
        dataFormat: format,
        databaseType,
        primaryKey: pkName,
      }).fetch();
      if (result === 'cancelled') {
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
    selectedCols,
    tableName,
    rows,
    selectedRows,
    scope,
    format,
    databaseType,
    dbSessionId,
    onClose,
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
            {scope === 'entire_table' && IS_DESKTOP_BUILD && (
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
        // DBR-06, restored: the cancel reports what happened to the partial
        // file, as the backend answered — deleted or kept are two different
        // messages. Only the SQL path reaches this state; provided exports
        // are one-shot and have no cancel button.
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
            <Select
              value={format}
              options={formatOptions}
              onChange={(v) => setFormat(v as ExportFormat)}
            />
          </div>

          <div>
            <label className="mb-1 block text-xs font-medium text-fg-secondary">
              {t('export.range')}
            </label>
            <Select value={scope} options={scopeOptions} onChange={handleScopeChange} />
          </div>

          <div>
            <div className="mb-1 flex items-center justify-between">
              <label className="text-xs font-medium text-fg-secondary">
                {t('export.selectColumns')}
              </label>
              <button
                type="button"
                className="text-xs text-accent hover:underline"
                onClick={toggleAll}
              >
                {selectedCols.size === loadedColumns.length
                  ? t('common.deselectAll')
                  : t('common.selectAll')}
              </button>
            </div>
            <div className="max-h-40 overflow-y-auto rounded-md border border-edge bg-surface p-2">
              {loadedColumns.map((col) => (
                <label
                  key={col.name}
                  className="flex cursor-pointer items-center gap-2 rounded px-1 py-0.5 hover:bg-surface-raised"
                >
                  <input
                    type="checkbox"
                    checked={selectedCols.has(col.name)}
                    onChange={() => toggleColumn(col.name)}
                    className="accent-accent"
                  />
                  <span className="text-xs text-fg-secondary">{col.name}</span>
                  <span className={cn('text-[10px]', dataTypeTextClass(col.dataType))}>
                    {col.dataType}
                  </span>
                </label>
              ))}
            </div>
          </div>

          <div className="rounded-md border border-edge bg-surface px-3 py-2 text-xs text-fg-muted">
            {t('export.willExport', { rows: rowCount, cols: selectedCols.size })},{' '}
            {t('export.formatAs')}{' '}
            <span className="font-medium text-fg-secondary">
              {OBJECT_EXPORT_FORMAT_OPTIONS.find((o) => o.value === format)?.label}
            </span>
          </div>

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
