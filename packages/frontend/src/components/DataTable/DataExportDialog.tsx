import { useCallback, useEffect, useMemo, useState } from 'react';
import { Dialog } from '../ui/Dialog';
import { Button } from '../ui/Button';
import { Select } from '../ui/Select';
import { CopyableError } from '../ui/CopyableError';
import { ExportSuccessPanel } from '../export/ExportSuccessPanel';
import { generateExportFromArrays } from '../../lib/exportData';
import type { ExportFormat, ExportScope } from '../../lib/exportData';
import {
  ARRAY_EXPORT_FORMAT_OPTIONS,
  buildExportScopeOptions,
  saveExportResultWithDialog,
} from '../../lib/exportDialogShared';
import { isStreamableExportFormat, streamTableExportToSaveDialog } from '../../lib/exportStream';
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
  const [status, setStatus] = useState<'form' | 'exporting' | 'success'>('form');
  const [exporting, setExporting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    setStatus('form');
    setError(null);
    // The caller's preferred scope (DB-15) — reset on every open, so a
    // manual export after a filter-panel export starts from the same default.
    setScope(initialScope ?? 'current_page');
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

  const formatOptions = useMemo(() => {
    if (scope !== 'entire_table' && scope !== 'filtered_table') return ARRAY_EXPORT_FORMAT_OPTIONS;
    return ARRAY_EXPORT_FORMAT_OPTIONS.filter((o) => o.value !== 'xlsx');
  }, [scope]);

  const handleScopeChange = useCallback(
    (v: string) => {
      const next = v as ExportScope;
      setScope(next);
      if (
        (next === 'entire_table' || next === 'filtered_table') &&
        !isStreamableExportFormat(format)
      ) {
        setFormat('csv');
      }
    },
    [format],
  );

  const handleExport = useCallback(async () => {
    if (exportLocked) return;
    setError(null);
    setExporting(true);
    setStatus('exporting');
    try {
      const columnNames = columns.map((c) => c.name);
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
          columns: columnNames,
          format,
          databaseType,
          pkName: primaryKeyColumns?.[0],
          ...(sql ? { sql } : {}),
        });
        if (result === 'cancelled') {
          setStatus('form');
          setExporting(false);
          return;
        }
        setStatus('success');
        return;
      }

      const result = generateExportFromArrays({
        columnNames,
        rows,
        selectedRows,
        scope,
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
              disabled={
                exporting ||
                exportLocked ||
                (scope !== 'entire_table' && scope !== 'filtered_table' && rows.length === 0) ||
                (scope === 'selected' && selectedRows.size === 0)
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

          <div className="rounded-md border border-edge bg-surface px-3 py-2 text-xs text-fg-muted">
            {t('export.willExport', { rows: rowCount, cols: columns.length })},{' '}
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
