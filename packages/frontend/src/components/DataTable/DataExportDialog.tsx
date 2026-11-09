import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Dialog } from '../ui/Dialog';
import { Button } from '../ui/Button';
import { Select } from '../ui/Select';
import { CopyableError } from '../ui/CopyableError';
import { ExportSuccessPanel } from '../export/ExportSuccessPanel';
import { ExportColumnPicker } from '../export/ExportColumnPicker';
import type { ExportFormat, ExportScope } from '../../lib/exportData';
import {
  ARRAY_EXPORT_FORMAT_OPTIONS,
  buildExportFormatOptions,
  buildExportScopeOptions,
} from '../../lib/exportDialogShared';
import { IS_DESKTOP_BUILD } from '../../platform/env';
import { exportSingleTableViaRust } from '../../lib/batchExportJob';
import { resolveDataFetcher } from '../../lib/dataFetcher';
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
  /** Columns the export writes, in display order (DBR-06, restored). */
  const [selectedCols, setSelectedCols] = useState<Set<string>>(
    () => new Set(columns.map((c) => c.name)),
  );
  const [status, setStatus] = useState<'form' | 'exporting' | 'success' | 'aborted'>('form');
  const [exporting, setExporting] = useState(false);
  /** The cancel was asked for; the run is winding down (DBR-06, restored). */
  const [cancelling, setCancelling] = useState(false);
  /** Whether the aborted run's partial file was deleted (reported honestly). */
  const [partialDeleted, setPartialDeleted] = useState(false);
  const [error, setError] = useState<string | null>(null);
  /** Cancel handle of the running SQL export; null on the provided path (no cancel there). */
  const cancelExportRef = useRef<(() => void) | null>(null);

  const isStreamingScope = scope === 'entire_table' || scope === 'filtered_table';

  useEffect(() => {
    if (!open) return;
    setStatus('form');
    setError(null);
    setCancelling(false);
    setPartialDeleted(false);
    cancelExportRef.current = null;
    // The caller's preferred scope (DB-15) — reset on every open, so a
    // manual export after a filter-panel export starts from the same default.
    setScope(initialScope ?? 'current_page');
    // The column default is "every column" on every open (DBR-06).
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

  /** The picked columns, in display order — the order the export writes. */
  const selectedColumnNames = useMemo(
    () => columns.filter((c) => selectedCols.has(c.name)).map((c) => c.name),
    [columns, selectedCols],
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

  // DBR-06, restored for the new architecture: the format list carries its
  // own availability — SQL UPDATE needs a key column, web streaming scopes
  // (entire table / filter results) only serve CSV — and an unavailable
  // format falls back to CSV instead of keeping a disabled selection. The
  // disabled option stays visible with its reason. XLSX is never disabled
  // any more: JS SheetJS and the Rust BinaryFormatter both serve it.
  const hasKeyColumn = (primaryKeyColumns?.length ?? 0) > 0;
  const formatOptions = useMemo(
    () =>
      buildExportFormatOptions({
        options: ARRAY_EXPORT_FORMAT_OPTIONS,
        scope,
        isDesktop: IS_DESKTOP_BUILD,
        hasKeyColumn,
      }),
    [scope, hasKeyColumn],
  );

  // An unavailable format (scope became streaming, or no key column) falls
  // back to CSV instead of keeping a disabled selection — the disabled
  // option stays visible with its reason.
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
    if (exportLocked) return;
    setError(null);
    setExporting(true);
    setStatus('exporting');
    setCancelling(false);
    setPartialDeleted(false);
    cancelExportRef.current = null;
    try {
      if (scope === 'entire_table' || scope === 'filtered_table') {
        if (!dbSessionId) throw new Error('Missing connection');
        const sql =
          scope === 'filtered_table'
            ? await onBuildFilteredExportSql?.().then((statement) => statement)
            : undefined;
        if (scope === 'filtered_table' && !sql) {
          throw new Error(t('export.filteredScopeUnavailable'));
        }
        // Item 3b: entire/filtered rows stream through the Rust batch
        // exporter (single-table request; filtered passes the backend's
        // own statement as a SQL source) — no frontend streaming path.
        // The run is cancellable mid-flight (DBR-06, restored): the
        // handle arrives before the export starts.
        const result = await exportSingleTableViaRust({
          dbSessionId,
          tableName,
          // DBR-06: only the picked columns, in display order.
          columns: selectedColumnNames,
          dataFormat: format,
          databaseType,
          primaryKey: primaryKeyColumns?.[0],
          ...(sql ? { sql } : {}),
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
      // used to sit here is gone; entire/filtered above still require one.
      const scopedRows =
        scope === 'selected' && selectedRows.size > 0
          ? rows.filter((_, i) => selectedRows.has(i))
          : rows;
      // DBR-06: the provided rows are projected onto the picked columns, in
      // display order — a column subset must not shift the remaining values
      // into the wrong positions.
      const selectedIndexes = selectedColumnNames.map((name) =>
        columns.findIndex((c) => c.name === name),
      );
      const result = await resolveDataFetcher({
        tableName,
        scope,
        columns: columns
          .filter((c) => selectedCols.has(c.name))
          .map((c) => ({
            name: c.name,
            dataType: c.type ?? 'text',
            nullable: c.nullable ?? true,
          })),
        rows: scopedRows.map((row) => selectedIndexes.map((i) => row[i])),
        dataFormat: format,
        databaseType,
        primaryKey: primaryKeyColumns?.[0],
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
    columns,
    rows,
    selectedRows,
    selectedColumnNames,
    selectedCols,
    scope,
    format,
    tableName,
    databaseType,
    dbSessionId,
    primaryKeyColumns,
    exportLocked,
    onBuildFilteredExportSql,
    t,
    onClose,
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
            {isStreamingScope && IS_DESKTOP_BUILD && (
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
              {ARRAY_EXPORT_FORMAT_OPTIONS.find((o) => o.value === format)?.label}
            </span>
          </div>

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
