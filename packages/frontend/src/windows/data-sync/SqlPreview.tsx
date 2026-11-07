import { useCallback, useEffect, useRef, useState } from 'react';
import { Copy } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { ErrorBanner } from '../../components/ui/ErrorBanner';
import { Spinner } from '../../components/ui/Spinner';
import { useCopyFeedback } from '../../components/ui/useCopyFeedback';
import { useI18n } from '../../hooks/useI18n';
import type {
  DataSyncOperation,
  DataSyncSelectedRow,
  DataSyncTableSelection,
  DataSyncSqlStatement,
  SyncOptions,
} from '../../commands/sync';
import { syncCommands } from '../../commands/sync';
import { filterStatementsByOp, statementsToPreviewText } from './clientSqlPreview';
import type { DataSyncTableResult } from './mappingView';

type OpFilter = 'all' | DataSyncOperation;

/** How long the "copied" button label stays before reverting to "copy". */
const COPIED_FEEDBACK_MS = 2000;

interface SqlPreviewProps {
  sourceConnId: string;
  targetConnId: string;
  sourceDatabase: string;
  targetDatabase: string;
  sourceSchema: string;
  targetSchema: string;
  tables: DataSyncTableResult[];
  options: SyncOptions;
  selectedRows?: DataSyncSelectedRow[];
  tableSelections?: DataSyncTableSelection[];
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
  selectedRows,
  tableSelections,
}: SqlPreviewProps) {
  const { t } = useI18n();
  const [opFilter, setOpFilter] = useState<OpFilter>('all');
  const [statements, setStatements] = useState<DataSyncSqlStatement[] | null>(null);
  const [previewError, setPreviewError] = useState('');
  const generation = useRef(0);
  const [loading, setLoading] = useState(false);
  const { copied, copy } = useCopyFeedback(COPIED_FEEDBACK_MS);

  const loadPreview = useCallback(async () => {
    const revision = ++generation.current;
    setLoading(true);
    setStatements(null);
    setPreviewError('');
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
        selectedRows,
        ...(tableSelections?.length ? [tableSelections] : []),
      );
      if (revision !== generation.current) return;
      setStatements(stmts);
    } catch (error) {
      if (revision !== generation.current) return;
      setStatements(null);
      setPreviewError(error instanceof Error ? error.message : String(error));
    } finally {
      if (revision === generation.current) setLoading(false);
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
    selectedRows,
    tableSelections,
  ]);

  useEffect(() => {
    void loadPreview();
    return () => {
      generation.current += 1;
    };
  }, [loadPreview]);

  const previewText = statements
    ? statementsToPreviewText(filterStatementsByOp(statements, opFilter), opFilter)
    : '';
  const isPreviewSizeLimitError = previewError.includes(
    'Data Sync SQL preview exceeds the 16 MiB IPC limit',
  );

  const handleCopy = () => copy(previewText);

  const filters: OpFilter[] = ['all', 'INSERT', 'UPDATE', 'DELETE'];

  return (
    <div data-testid="data-sync-preview" className="flex min-h-0 flex-1 flex-col">
      <div className="flex shrink-0 items-center gap-2 border-b border-edge px-3 py-2">
        <span className="text-xs font-semibold uppercase tracking-wider text-fg-muted">
          {t('common.sqlPreviewLower')}
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
        {loading && <Spinner size="md" tone="muted" />}
        <Button variant="ghost" size="sm" onClick={() => void loadPreview()}>
          {t('sync.refreshPreview')}
        </Button>
        <Button variant="secondary" size="sm" onClick={handleCopy}>
          <Copy className="h-3.5 w-3.5" />
          {copied ? t('common.copied') : t('common.copy')}
        </Button>
      </div>
      {previewError && (
        <ErrorBanner className="p-3 text-sm">
          {isPreviewSizeLimitError ? t('sync.sqlPreviewLimitReached') : previewError}
        </ErrorBanner>
      )}
      <pre className="min-h-0 flex-1 overflow-auto p-3 font-mono text-[11px] leading-relaxed text-fg-secondary">
        {previewText}
      </pre>
    </div>
  );
}
