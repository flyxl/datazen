import { useCallback, useMemo, useState } from 'react';
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
import type { ColumnSchema } from '../../types';
import { useI18n } from '../../hooks/useI18n';

interface ExportDialogProps {
  open: boolean;
  onClose: () => void;
  tableName: string;
  columns: ColumnSchema[];
  rows: Record<string, unknown>[];
  selectedRows: Set<number>;
  databaseType?: string;
}

export function ExportDialog({ open, onClose, tableName, columns, rows, selectedRows, databaseType }: ExportDialogProps) {
  const { t } = useI18n();
  const [format, setFormat] = useState<ExportFormat>('csv');
  const [scope, setScope] = useState<ExportScope>('current_page');
  const [selectedCols, setSelectedCols] = useState<Set<string>>(() => new Set(columns.map((c) => c.name)));
  const [exporting, setExporting] = useState(false);
  const [error, setError] = useState<string | null>(null);

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
      if (prev.size === columns.length) return new Set();
      return new Set(columns.map((c) => c.name));
    });
  }, [columns]);

  const rowCount = useMemo(() => {
    if (scope === 'selected' && selectedRows.size > 0) return selectedRows.size;
    return rows.length;
  }, [scope, selectedRows, rows]);

  const scopeOptions = useMemo(
    () => buildExportScopeOptions(t, rows.length, selectedRows.size),
    [t, rows.length, selectedRows.size],
  );

  const handleExport = useCallback(async () => {
    setError(null);
    setExporting(true);
    try {
      const colNames = columns.map((c) => c.name).filter((n) => selectedCols.has(n));
      const result = generateExport({
        tableName,
        columns,
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
  }, [columns, selectedCols, tableName, rows, selectedRows, scope, format, databaseType, onClose]);

  return (
    <Dialog
      open={open}
      title={t('export.title')}
      description={t('export.description', { table: tableName })}
      onClose={onClose}
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>{t('common.cancel')}</Button>
          <Button
            variant="primary"
            onClick={() => void handleExport()}
            disabled={exporting || selectedCols.size === 0}
          >
            {exporting ? t('export.exporting') : t('export.export')}
          </Button>
        </>
      }
    >
      <div className="space-y-4">
        {/* Format */}
        <div>
          <label className="mb-1 block text-xs font-medium text-fg-secondary">{t('export.format')}</label>
          <Select
            value={format}
            options={OBJECT_EXPORT_FORMAT_OPTIONS}
            onChange={(v) => setFormat(v as ExportFormat)}
          />
        </div>

        {/* Scope */}
        <div>
          <label className="mb-1 block text-xs font-medium text-fg-secondary">{t('export.range')}</label>
          <Select
            value={scope}
            options={scopeOptions}
            onChange={(v) => setScope(v as ExportScope)}
          />
        </div>

        {/* Column selection */}
        <div>
          <div className="mb-1 flex items-center justify-between">
            <label className="text-xs font-medium text-fg-secondary">{t('export.selectColumns')}</label>
            <button type="button" className="text-xs text-accent hover:underline" onClick={toggleAll}>
              {selectedCols.size === columns.length ? t('common.deselectAll') : t('common.selectAll')}
            </button>
          </div>
          <div className="max-h-40 overflow-y-auto rounded-md border border-edge bg-surface p-2">
            {columns.map((col) => (
              <label key={col.name} className="flex cursor-pointer items-center gap-2 rounded px-1 py-0.5 hover:bg-surface-raised">
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

        {/* Summary */}
        <div className="rounded-md border border-edge bg-surface px-3 py-2 text-xs text-fg-muted">
          {t('export.willExport', { rows: rowCount, cols: selectedCols.size })}
          , {t('export.formatAs')}{' '}
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
