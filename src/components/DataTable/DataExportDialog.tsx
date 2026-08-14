import { useCallback, useMemo, useState } from 'react';
import { Dialog } from '../ui/Dialog';
import { Button } from '../ui/Button';
import { Select } from '../ui/Select';
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
  connectionId?: string;
  totalRows?: number;
  primaryKeyColumns?: string[];
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
  connectionId,
  totalRows,
  primaryKeyColumns,
  dataExportCapability = 'full_table',
}: DataExportDialogProps) {
  const { t } = useI18n();
  const exportLocked = dataExportCapability === 'none';
  const allowEntire =
    !exportLocked &&
    supportsFullTableExport(dataExportCapability) &&
    Boolean(connectionId) &&
    tableName !== 'data';
  const [format, setFormat] = useState<ExportFormat>('csv');
  const [scope, setScope] = useState<ExportScope>('current_page');
  const [exporting, setExporting] = useState(false);
  const [error, setError] = useState<string | null>(null);

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
    if (scope !== 'entire_table') return ARRAY_EXPORT_FORMAT_OPTIONS;
    return ARRAY_EXPORT_FORMAT_OPTIONS.filter((o) => o.value !== 'xlsx');
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
    if (exportLocked) return;
    setError(null);
    setExporting(true);
    try {
      const columnNames = columns.map((c) => c.name);
      if (scope === 'entire_table') {
        if (!connectionId) throw new Error('Missing connection');
        if (!isStreamableExportFormat(format)) throw new Error('xlsx');
        const result = await streamTableExportToSaveDialog({
          connectionId,
          tableName,
          columns: columnNames,
          format,
          databaseType,
          pkName: primaryKeyColumns?.[0],
        });
        if (result === 'cancelled') {
          setExporting(false);
          return;
        }
        onClose();
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
    columns,
    rows,
    selectedRows,
    scope,
    format,
    tableName,
    databaseType,
    connectionId,
    primaryKeyColumns,
    exportLocked,
    onClose,
  ]);

  return (
    <Dialog
      open={open}
      title={t('export.title')}
      description={tableName !== 'data' ? t('export.description', { table: tableName }) : undefined}
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
              exporting ||
              exportLocked ||
              (scope !== 'entire_table' && rows.length === 0) ||
              (scope === 'selected' && selectedRows.size === 0)
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

        <div className="rounded-md border border-edge bg-surface px-3 py-2 text-xs text-fg-muted">
          {t('export.willExport', { rows: rowCount, cols: columns.length })}, {t('export.formatAs')}{' '}
          <span className="font-medium text-fg-secondary">
            {ARRAY_EXPORT_FORMAT_OPTIONS.find((o) => o.value === format)?.label}
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
