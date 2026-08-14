import { useCallback, useEffect, useMemo, useState } from 'react';
import { Dialog } from '../../components/ui/Dialog';
import { Button } from '../../components/ui/Button';
import { Select } from '../../components/ui/Select';
import { generateExport } from '../../lib/exportData';
import type { ExportFormat, ExportScope } from '../../lib/exportData';
import {
  OBJECT_EXPORT_FORMAT_OPTIONS,
  buildExportScopeOptions,
  saveExportResultWithDialog,
} from '../../lib/exportDialogShared';
import { isStreamableExportFormat, streamTableExportToSaveDialog } from '../../lib/exportStream';
import { supportsFullTableExport, type DataExportCapability } from '../../lib/exportCapability';
import type { ColumnSchema } from '../../types';
import { useI18n } from '../../hooks/useI18n';
import { getCachedTableSchema } from '../../lib/schemaCache';

interface ExportDialogProps {
  open: boolean;
  onClose: () => void;
  tableName: string;
  columns: ColumnSchema[];
  rows: Record<string, unknown>[];
  selectedRows: Set<number>;
  databaseType?: string;
  connectionId?: string;
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
  connectionId,
  totalRows,
  defaultScope,
  dataExportCapability = 'full_table',
}: ExportDialogProps) {
  const { t } = useI18n();
  const exportLocked = dataExportCapability === 'none';
  const allowEntire =
    !exportLocked && supportsFullTableExport(dataExportCapability) && Boolean(connectionId);
  const [format, setFormat] = useState<ExportFormat>('csv');
  const [scope, setScope] = useState<ExportScope>(
    defaultScope ?? (allowEntire && rows.length === 0 ? 'entire_table' : 'current_page'),
  );
  const [loadedColumns, setLoadedColumns] = useState<ColumnSchema[]>(columns);
  const [selectedCols, setSelectedCols] = useState<Set<string>>(
    () => new Set(columns.map((c) => c.name)),
  );
  const [exporting, setExporting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setLoadedColumns(columns);
    setSelectedCols(new Set(columns.map((c) => c.name)));
    if (!open || !connectionId) return;
    let cancelled = false;
    void getCachedTableSchema(connectionId, tableName)
      .then((schema) => {
        if (cancelled || schema.columns.length === 0) return;
        setLoadedColumns(schema.columns);
        setSelectedCols(new Set(schema.columns.map((c) => c.name)));
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [open, connectionId, tableName, columns]);

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

  const formatOptions = useMemo(() => {
    if (scope !== 'entire_table') return OBJECT_EXPORT_FORMAT_OPTIONS;
    return OBJECT_EXPORT_FORMAT_OPTIONS.filter((o) => o.value !== 'xlsx');
  }, [scope]);

  const handleScopeChange = useCallback(
    (v: string) => {
      const next = v as ExportScope;
      setScope(next);
      if (next === 'entire_table' && !isStreamableExportFormat(format)) {
        setFormat('csv');
      }
    },
    [format],
  );

  const handleExport = useCallback(async () => {
    setError(null);
    setExporting(true);
    try {
      const colNames = loadedColumns.map((c) => c.name).filter((n) => selectedCols.has(n));
      if (scope === 'entire_table') {
        if (!connectionId) {
          throw new Error('Missing connection');
        }
        if (!isStreamableExportFormat(format)) {
          throw new Error('xlsx');
        }
        const pkName = loadedColumns.find((c) => c.isPrimaryKey)?.name;
        const result = await streamTableExportToSaveDialog({
          connectionId,
          tableName,
          columns: colNames,
          format,
          databaseType,
          pkName,
        });
        if (result === 'cancelled') {
          setExporting(false);
          return;
        }
        onClose();
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
        setExporting(false);
        return;
      }
      onClose();
    } catch (e) {
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
    connectionId,
    onClose,
  ]);

  return (
    <Dialog
      open={open}
      title={t('export.title')}
      description={t('export.description', { table: tableName })}
      onClose={onClose}
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
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
      }
    >
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
                <span className="text-[10px] text-fg-muted">{col.dataType}</span>
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
          <div className="rounded-md border border-red-500/20 bg-red-500/10 px-3 py-2 text-xs text-red-400">
            {error}
          </div>
        )}
      </div>
    </Dialog>
  );
}
