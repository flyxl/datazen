import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Dialog } from '../ui/Dialog';
import { Button } from '../ui/Button';
import { Select } from '../ui/Select';
import { CopyableError } from '../ui/CopyableError';
import { ExportSuccessPanel } from '../export/ExportSuccessPanel';
import { ExportColumnPicker } from '../export/ExportColumnPicker';
import { generateExportFromArrays } from '../../lib/exportData';
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
import { useI18n } from '../../hooks/useI18n';
import type { ColumnDef } from './TableHeader';

interface DataExportDialogProps {
  open: boolean;
  onClose: () => void;
  columns: ColumnDef[];
  rows: unknown[][];
  selectedRows?: Set<number>;
  tableName?: string;
  databaseType?: string;
  dbSessionId?: string;
  totalRows?: number;
  primaryKeyColumns?: string[];
  /**
   * The scope the dialog opens with (DB-15: the filter panel's "Export…" exit
   * preselects `filtered_table`). Ignored on re-open when the caller keeps it
   * stable — the effect keys on `open`.
   */
  initialScope?: ExportScope;
  /**
   * The statements builder of "the current filter results, streamed" (DB-15).
   *
   * Provided only when a filtered statement exists — the scope never appears
   * for a table whose filtered read cannot be built. The statement is the
   * backend's own (`previewTableReadSql` with `unbounded`), so what is
   * exported is what the filters really select.
   */
  onBuildFilteredExportSql?: () => Promise<string>;
  /**
   * Driver data-export capability. Defaults to `full_table`.
   * - `none`: export disabled entirely.
   * - `loaded_only`: only already-loaded rows may be exported (no entire-table).
   */
  dataExportCapability?: DataExportCapability;
}

export function DataExportDialog({
  open,
  onClose,
  columns,
  rows,
  selectedRows = new Set<number>(),
  tableName = 'data',
  databaseType,
  dbSessionId,
  totalRows,
  primaryKeyColumns,
  initialScope,
  onBuildFilteredExportSql,
  dataExportCapability = 'full_table',
}: DataExportDialogProps) {
  const { t } = useI18n();
  const exportLocked = dataExportCapability === 'none';
  const allowEntire =
    !exportLocked &&
    supportsFullTableExport(dataExportCapability) &&
    Boolean(dbSessionId) &&
    tableName !== 'data';
  /** The filtered scope exists exactly when its statement builder does (DB-15). */
  const allowFiltered = !exportLocked && Boolean(dbSessionId) && onBuildFilteredExportSql != null;
  const [format, setFormat] = useState<ExportFormat>('csv');
  const [scope, setScope] = useState<ExportScope>('current_page');
  const [status, setStatus] = useState<'form' | 'exporting' | 'success' | 'aborted'>('form');
  const [exporting, setExporting] = useState(false);
  /** The cancel was asked for; the run is winding down (DBR-06). */
  const [cancelling, setCancelling] = useState(false);
  /** Rows written so far by a streaming export (DBR-06 行数进度). */
  const [rowsWritten, setRowsWritten] = useState(0);
  /** Whether the aborted run's partial file was deleted (reported honestly). */
  const [partialDeleted, setPartialDeleted] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // DBR-06 列选择: default = every column; the export writes the subset in
  // display order (the grid's `columns` prop is the display order).
  const [selectedCols, setSelectedCols] = useState<Set<string>>(
    () => new Set(columns.map((c) => c.name)),
  );
  const runHandleRef = useRef<TableExportRunHandle | null>(null);

  const isStreamingScope = scope === 'entire_table' || scope === 'filtered_table';

  useEffect(() => {
    if (!open) return;
    setStatus('form');
    setError(null);
    setCancelling(false);
    setRowsWritten(0);
    // The caller's preferred scope (DB-15) — reset on every open, so a
    // manual export after a filter-panel export starts from the same default.
    setScope(initialScope ?? 'current_page');
    // The column default is "every column" on every open (DBR-06 恢复默认).
    setSelectedCols(new Set(columns.map((c) => c.name)));
    // The preferred scope may not be served by this driver; the effect that
    // renders the options drops an impossible value on its own below.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  // A preferred scope the driver does not serve would leave the select
  // pointing at a missing option; fall back to the page scope, which every
  // driver serves.
  useEffect(() => {
    if (scope === 'entire_table' && !allowEntire) setScope('current_page');
    if (scope === 'filtered_table' && !allowFiltered) setScope('current_page');
  }, [allowEntire, allowFiltered, scope]);

  const selectedColumnNames = useMemo(
    () => columns.filter((c) => selectedCols.has(c.name)).map((c) => c.name),
    [columns, selectedCols],
  );

  // DBR-06 格式统一: both dialogs render the same list from
  // `buildExportFormatOptions`; XLSX is greyed out with its reason for the
  // streaming scopes instead of vanishing, and SQL UPDATE needs a key column.
  const hasKeyColumn = (primaryKeyColumns?.length ?? 0) > 0;
  const formatOptions = useMemo(
    () => buildExportFormatOptions({ streamingScope: isStreamingScope, hasKeyColumn }),
    [isStreamingScope, hasKeyColumn],
  );

  // An unavailable format (scope became streaming, or no key column) falls
  // back to CSV instead of keeping a disabled selection (范围变化时自动清理
  // 不合法选项) — the disabled option stays visible with its reason.
  useEffect(() => {
    if (formatOptions.find((o) => o.value === format)?.disabled) setFormat('csv');
  }, [formatOptions, format]);

  const rowCount = useMemo(() => {
    if (scope === 'entire_table' || scope === 'filtered_table') return totalRows ?? rows.length;
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
        allowFiltered,
      ),
    [t, rows.length, selectedRows.size, allowEntire, allowFiltered, totalRows],
  );

  const handleScopeChange = useCallback(
    (v: string) => {
      const next = v as ExportScope;
      setScope(next);
      setRowsWritten(0);
      if (
        (next === 'entire_table' || next === 'filtered_table') &&
        !isStreamableExportFormat(format)
      ) {
        setFormat('csv');
      }
    },
    [format],
  );

  const handleFormatChange = useCallback(
    (v: string) => {
      const next = v as ExportFormat;
      // A disabled option (XLSX on a streaming scope, SQL UPDATE without a key)
      // can still arrive from keyboard input on the raw select — ignore it.
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
    setSelectedCols(new Set(columns.map((c) => c.name)));
  }, [columns]);

  const handleCancelExport = useCallback(() => {
    if (cancelling) return;
    setCancelling(true);
    runHandleRef.current?.cancel();
  }, [cancelling]);

  const handleExport = useCallback(async () => {
    if (exportLocked) return;
    setError(null);
    setExporting(true);
    setStatus('exporting');
    setCancelling(false);
    setRowsWritten(0);
    runHandleRef.current = null;
    try {
      if (scope === 'entire_table' || scope === 'filtered_table') {
        if (!dbSessionId) throw new Error('Missing connection');
        if (!isStreamableExportFormat(format)) throw new Error('xlsx');
        const sql =
          scope === 'filtered_table'
            ? await onBuildFilteredExportSql?.().then((statement) => statement)
            : undefined;
        if (scope === 'filtered_table' && !sql) {
          throw new Error(t('export.filteredScopeUnavailable'));
        }
        const result = await streamTableExportToSaveDialog({
          dbSessionId,
          tableName,
          // DBR-06: only the picked columns, in display order. For the
          // filtered scope the streamed statement is the backend's own
          // full-column read; the streamer writes the subset by name.
          columns: selectedColumnNames,
          format,
          databaseType,
          pkName: primaryKeyColumns?.[0],
          ...(sql ? { sql } : {}),
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

      const result = generateExportFromArrays({
        columnNames: columns.map((c) => c.name),
        rows,
        selectedRows,
        scope,
        selectedColumns: selectedColumnNames,
        primaryKeyColumns,
        format,
        tableName,
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
    columns,
    rows,
    selectedRows,
    selectedColumnNames,
    scope,
    format,
    tableName,
    databaseType,
    dbSessionId,
    primaryKeyColumns,
    exportLocked,
    onBuildFilteredExportSql,
    t,
  ]);

  return (
    <Dialog
      open={open}
      testId="data-export-dialog"
      title={t('common.exportData')}
      description={tableName !== 'data' ? t('export.description', { table: tableName }) : undefined}
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
                exporting ||
                exportLocked ||
                (scope !== 'entire_table' && scope !== 'filtered_table' && rows.length === 0) ||
                (scope === 'selected' && selectedRows.size === 0) ||
                // An empty column selection exports nothing; the button says
                // so instead of writing a header-only file (DBR-06).
                selectedColumnNames.length === 0
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
            columns={columns.map((c) => ({
              name: c.name,
              type: c.type,
              isPrimaryKey: primaryKeyColumns?.includes(c.name) ?? false,
            }))}
            selected={selectedCols}
            onToggle={toggleColumn}
            onSelectAll={() => setSelectedCols(new Set(columns.map((c) => c.name)))}
            onClearAll={() => setSelectedCols(new Set())}
            onResetDefault={resetColumnsToDefault}
          />

          <div className="rounded-md border border-edge bg-surface px-3 py-2 text-xs text-fg-muted">
            {t('export.willExport', { rows: rowCount, cols: selectedColumnNames.length })},{' '}
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
              className="rounded-md border border-danger/20 bg-danger/10 px-3 py-2 text-xs text-danger"
            />
          )}
        </div>
      )}
    </Dialog>
  );
}
