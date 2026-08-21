import { useCallback, useEffect, useState } from 'react';
import { Copy, Loader2 } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { useI18n } from '../../hooks/useI18n';
import type { DataSyncOperation, DataSyncSqlStatement, SyncOptions } from '../../commands/sync';
import { syncCommands } from '../../commands/sync';
import {
  buildClientSqlPreview,
  filterStatementsByOp,
  statementsToPreviewText,
} from './clientSqlPreview';
import type { DataSyncTableResult } from './mappingView';

type OpFilter = 'all' | DataSyncOperation;

interface SqlPreviewProps {
  sourceConnId: string;
  targetConnId: string;
  sourceDatabase: string;
  targetDatabase: string;
  sourceSchema: string;
  targetSchema: string;
  tables: DataSyncTableResult[];
  options: SyncOptions;
}

export function SqlPreview({
  sourceConnId,
  targetConnId,
  sourceDatabase,
  targetDatabase,
  sourceSchema,
  targetSchema,
  tables,
  options,
}: SqlPreviewProps) {
  const { t } = useI18n();
  const [opFilter, setOpFilter] = useState<OpFilter>('all');
  const [statements, setStatements] = useState<DataSyncSqlStatement[] | null>(null);
  const [clientText, setClientText] = useState('');
  const [loading, setLoading] = useState(false);
  const [copied, setCopied] = useState(false);

  const loadPreview = useCallback(async () => {
    setLoading(true);
    try {
      const stmts = await syncCommands.generateDataSyncSql(
        sourceConnId,
        targetConnId,
        tables,
        options,
        sourceDatabase,
        targetDatabase,
        sourceSchema || undefined,
        targetSchema || undefined,
      );
      setStatements(stmts);
      setClientText('');
    } catch {
      setStatements(null);
      setClientText(buildClientSqlPreview(tables, options));
    } finally {
      setLoading(false);
    }
  }, [
    sourceConnId,
    targetConnId,
    tables,
    options,
    sourceDatabase,
    targetDatabase,
    sourceSchema,
    targetSchema,
  ]);

  useEffect(() => {
    void loadPreview();
  }, [loadPreview]);

  const previewText = statements
    ? statementsToPreviewText(filterStatementsByOp(statements, opFilter), opFilter)
    : clientText;

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(previewText);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      /* ignore */
    }
  };

  const filters: OpFilter[] = ['all', 'INSERT', 'UPDATE', 'DELETE'];

  return (
    <div data-testid="data-sync-preview" className="flex min-h-0 flex-1 flex-col">
      <div className="flex shrink-0 items-center gap-2 border-b border-edge px-3 py-2">
        <span className="text-xs font-semibold uppercase tracking-wider text-fg-muted">
          {t('sync.sqlPreviewTitle')}
        </span>
        <div className="flex flex-wrap gap-1">
          {filters.map((f) => (
            <Button
              key={f}
              variant={opFilter === f ? 'secondary' : 'ghost'}
              size="sm"
              className="text-[10px]"
              onClick={() => setOpFilter(f)}
            >
              {f === 'all' ? t('sync.filter.all') : f}
            </Button>
          ))}
        </div>
        <div className="flex-1" />
        {loading && <Loader2 className="h-3.5 w-3.5 animate-spin text-fg-muted" />}
        <Button variant="ghost" size="sm" onClick={() => void loadPreview()}>
          {t('sync.refreshPreview')}
        </Button>
        <Button variant="secondary" size="sm" onClick={() => void handleCopy()}>
          <Copy className="h-3.5 w-3.5" />
          {copied ? t('common.copied') : t('common.copy')}
        </Button>
      </div>
      <pre className="min-h-0 flex-1 overflow-auto p-3 font-mono text-[11px] leading-relaxed text-fg-secondary">
        {previewText}
      </pre>
    </div>
  );
}
