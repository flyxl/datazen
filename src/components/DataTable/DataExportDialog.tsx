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
}

export function DataExportDialog({
  open,
  onClose,
  columns,
  rows,
  selectedRows = new Set<number>(),
  tableName = 'data',
  databaseType,
}: DataExportDialogProps) {
  const { t } = useI18n();
  const [format, setFormat] = useState<ExportFormat>('csv');
  const [scope, setScope] = useState<ExportScope>('current_page');
  const [exporting, setExporting] = useState(false);
  const [error, setError] = useState<string | null>(null);

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
      const columnNames = columns.map((c) => c.name);
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
  }, [columns, rows, selectedRows, scope, format, tableName, databaseType, onClose]);

  return (
    <Dialog
      open={open}
      title={t('export.title')}
      description={tableName !== 'data' ? t('export.description', { table: tableName }) : undefined}
      onClose={onClose}
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>{t('common.cancel')}</Button>
          <Button
            variant="primary"
            onClick={() => void handleExport()}
            disabled={exporting || rows.length === 0}
          >
            {exporting ? t('export.exporting') : t('export.export')}
          </Button>
        </>
      }
    >
      <div className="space-y-4">
        <div>
          <label className="mb-1 block text-xs font-medium text-fg-secondary">{t('export.format')}</label>
          <Select
            value={format}
            options={ARRAY_EXPORT_FORMAT_OPTIONS}
            onChange={(v) => setFormat(v as ExportFormat)}
          />
        </div>

        <div>
          <label className="mb-1 block text-xs font-medium text-fg-secondary">{t('export.range')}</label>
          <Select
            value={scope}
            options={scopeOptions}
            onChange={(v) => setScope(v as ExportScope)}
          />
        </div>

        <div className="rounded-md border border-edge bg-surface px-3 py-2 text-xs text-fg-muted">
          {t('export.willExport', { rows: rowCount, cols: columns.length })}
          , {t('export.formatAs')}{' '}
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
