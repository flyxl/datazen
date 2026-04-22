import { useCallback, useEffect, useState } from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import { AlertTriangle, FileText, Loader2 } from 'lucide-react';
import { Dialog } from '../../components/ui/Dialog';
import { Button } from '../../components/ui/Button';
import { fileCommands } from '../../commands/file';
import { parseImportData, generateInsertSQL } from '../../lib/importData';
import type { ParsedData } from '../../lib/importData';
import { queryCommands } from '../../commands/query';
import { cn } from '../../lib/cn';
import { useI18n } from '../../hooks/useI18n';

interface ImportDialogProps {
  open: boolean;
  onClose: () => void;
  connectionId: string;
  tableName: string | null;
  onImported: () => void;
  databaseType?: string;
}

export function ImportDialog({ open: isOpen, onClose, connectionId, tableName, onImported, databaseType }: ImportDialogProps) {
  const { t } = useI18n();
  const [filePath, setFilePath] = useState<string | null>(null);
  const [parsedData, setParsedData] = useState<ParsedData | null>(null);
  const [importing, setImporting] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<string | null>(null);
  const [targetTable, setTargetTable] = useState(tableName ?? '');

  useEffect(() => {
    if (tableName) setTargetTable(tableName);
  }, [tableName]);

  const handlePickFile = useCallback(async () => {
    setError(null);
    setParsedData(null);
    setResult(null);
    try {
      const selected = await open({
        multiple: false,
        filters: [
          { name: 'Data Files', extensions: ['csv', 'json'] },
        ],
      });
      if (!selected) return;
      setFilePath(selected);
      setLoading(true);

      const content = await fileCommands.readFile(selected);
      const ext: 'csv' | 'json' = selected.toLowerCase().endsWith('.json') ? 'json' : 'csv';
      const data = parseImportData(content, ext);
      setParsedData(data);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  const handleImport = useCallback(async () => {
    if (!parsedData || !targetTable.trim()) return;
    setError(null);
    setResult(null);
    setImporting(true);
    try {
      const sql = generateInsertSQL(targetTable.trim(), parsedData, databaseType);
      if (!sql) {
        setError(t('import.noData'));
        return;
      }

      const queryResult = await queryCommands.executeQuery(connectionId, sql);
      const totalAffected = queryResult.results.reduce(
        (sum: number, r: { rowsAffected?: number }) => sum + (r.rowsAffected ?? 0),
        0,
      );
      setResult(t('import.success', { count: totalAffected, table: targetTable.trim() }));
      onImported();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setImporting(false);
    }
  }, [parsedData, targetTable, connectionId, onImported, databaseType, t]);

  return (
    <Dialog
      open={isOpen}
      title={t('import.title')}
      description={t('import.description')}
      onClose={onClose}
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>{t('common.cancel')}</Button>
          <Button
            variant="primary"
            onClick={() => void handleImport()}
            disabled={importing || !parsedData || !targetTable.trim()}
          >
            {importing ? t('import.importing') : t('import.import')}
          </Button>
        </>
      }
    >
      <div className="space-y-4">
        {/* File picker */}
        <div>
          <label className="mb-1 block text-xs font-medium text-fg-secondary">{t('import.selectFile')}</label>
          <Button variant="secondary" className="h-8 text-xs" onClick={() => void handlePickFile()}>
            <FileText className="h-3.5 w-3.5" />
            {filePath ? filePath.split('/').pop() : t('import.selectFile')}
          </Button>
        </div>

        {/* Target table */}
        <div>
          <label className="mb-1 block text-xs font-medium text-fg-secondary">{t('import.targetTable')}</label>
          <input
            type="text"
            value={targetTable}
            onChange={(e) => setTargetTable(e.target.value)}
            placeholder={t('import.tableNamePlaceholder')}
            className="h-9 w-full rounded-md border border-edge bg-surface px-3 text-sm text-fg outline-none focus:border-blue-500 focus:ring-2 focus:ring-blue-500/25"
          />
        </div>

        {loading && (
          <div className="flex items-center gap-2 text-xs text-fg-muted">
            <Loader2 className="h-4 w-4 animate-spin" />
            {t('import.parsing')}
          </div>
        )}

        {/* Preview */}
        {parsedData && (
          <div>
            <label className="mb-1 block text-xs font-medium text-fg-secondary">
              {t('common.preview')} ({parsedData.rows.length} {t('common.rows')}, {parsedData.columns.length} {t('common.columns')})
            </label>
            <div className="max-h-52 overflow-auto rounded-md border border-edge bg-surface">
              <table className="w-full border-collapse text-xs">
                <thead className="sticky top-0">
                  <tr className="bg-surface-alt">
                    {parsedData.columns.map((col) => (
                      <th key={col} className="border-b border-r border-edge px-2 py-1.5 text-left font-medium text-fg-secondary">
                        {col}
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {parsedData.rows.slice(0, 10).map((row, ri) => (
                    <tr key={ri} className={cn(ri % 2 === 0 ? 'bg-surface' : 'bg-surface-alt/30')}>
                      {parsedData.columns.map((col) => (
                        <td key={col} className="border-b border-r border-edge px-2 py-1 font-mono text-fg-secondary">
                          {row[col] === null ? (
                            <span className="italic text-fg-muted">NULL</span>
                          ) : (
                            typeof row[col] === 'object' ? JSON.stringify(row[col]) : String(row[col])
                          )}
                        </td>
                      ))}
                    </tr>
                  ))}
                </tbody>
              </table>
              {parsedData.rows.length > 10 && (
                <div className="border-t border-edge px-3 py-1.5 text-center text-[11px] text-fg-muted">
                  {t('import.rowsNotShown', { count: parsedData.rows.length - 10 })}
                </div>
              )}
            </div>
          </div>
        )}

        {/* Warning */}
        {parsedData && parsedData.rows.length > 1000 && (
          <div className="flex items-start gap-2 rounded-md border border-yellow-500/20 bg-yellow-500/10 px-3 py-2 text-xs text-yellow-600 dark:text-yellow-400">
            <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0" />
            <span>{t('import.largeImportRows', { count: parsedData.rows.length })}</span>
          </div>
        )}

        {result && (
          <div className="rounded-md border border-green-500/20 bg-green-500/10 px-3 py-2 text-xs text-green-600 dark:text-green-400">
            {result}
          </div>
        )}

        {error && (
          <div className="rounded-md border border-red-500/20 bg-red-500/10 px-3 py-2 text-xs text-red-400">
            {error}
          </div>
        )}
      </div>
    </Dialog>
  );
}
