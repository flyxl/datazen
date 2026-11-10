import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Dialog } from '../../components/ui/Dialog';
import { CopyableError } from '../../components/ui/CopyableError';
import { Button } from '../../components/ui/Button';
import { Select } from '../../components/ui/Select';
import { ExportSuccessPanel } from '../../components/export/ExportSuccessPanel';
import { ExportColumnPicker } from '../../components/export/ExportColumnPicker';
import type { ExportFormat, ExportScope } from '../../lib/exportData';
import {
  OBJECT_EXPORT_FORMAT_OPTIONS,
  buildExportFormatOptions,
  buildExportScopeOptions,
} from '../../lib/exportDialogShared';
import { IS_DESKTOP_BUILD } from '../../platform/env';
import { exportSingleTableViaRust } from '../../lib/batchExportJob';
import { resolveDataFetcher } from '../../lib/dataFetcher';
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

  const resetColumnsToDefault = useCallback(() => {
    setSelectedCols(new Set(loadedColumns.map((c) => c.name)));
  }, [loadedColumns]);

  // A preferred scope the driver does not serve would leave the select
  // pointing at a missing option; fall back to the page scope, which every
  // driver serves (same correction as DataExportDialog).
  useEffect(() => {
    if (scope === 'entire_table' && !allowEntire) setScope('current_page');
  }, [allowEntire, scope]);

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

  // DBR-06, restored for the new architecture: the format list carries its
  // own availability — SQL UPDATE needs a key column, web entire-table
  // only serves CSV — and an unavailable format falls back to CSV instead
  // of keeping a disabled selection. The disabled option stays visible
  // with its reason. XLSX is never disabled any more: JS SheetJS and the
  // Rust BinaryFormatter both serve it.
  const hasKeyColumn = loadedColumns.some((c) => c.isPrimaryKey);
  const formatOptions = useMemo(
    () =>
      buildExportFormatOptions({
        options: OBJECT_EXPORT_FORMAT_OPTIONS,
        scope,
        isDesktop: IS_DESKTOP_BUILD,
        hasKeyColumn,
      }),
    [scope, hasKeyColumn],
  );

  // An unavailable format (scope became entire-table on web, or no key
  // column) falls back to CSV instead of keeping a disabled selection —
  // the disabled option stays visible with its reason.
  useEffect(() => {
    if (formatOptions.find((o) => o.value === format)?.disabled) setFormat('csv');
  }, [formatOptions, format]);

  /** The reasons of the currently disabled formats, each shown once. */
  const disabledFormatReasons = useMemo(
    () => [
      ...new Set(
        formatOptions.flatMap((o) =>
          o.disabled && o.disabledReasonKey ? [o.disabledReasonKey] : [],
        ),
      ),
    ],
    [formatOptions],
  );

  const handleFormatChange = useCallback(
    (v: string) => {
      const next = v as ExportFormat;
      // A disabled option can still arrive from keyboard input on the raw
      // select — ignore it.
      if (formatOptions.find((o) => o.value === next)?.disabled) return;
      setFormat(next);
    },
    [formatOptions],
  );

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
            <Select value={format} options={formatOptions} onChange={handleFormatChange} />
            {/* Disabled with reason (DBR-06): an unavailable format stays
             * listed and greyed out, and its reason is on screen the whole
             * time the range makes it unavailable — not only while it is
             * selected. */}
            {disabledFormatReasons.map((reasonKey) => (
              <p
                key={reasonKey}
                data-testid="export-format-disabled-reason"
                className="mt-1 text-xs text-fg-muted"
              >
                {t(reasonKey)}
              </p>
            ))}
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
