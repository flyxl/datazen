import { Spinner } from '../../components/ui/Spinner';
import type {
  DataSyncSelectedRow,
  DataSyncTableResult,
  DataSyncTableSelection,
  SyncOptions,
} from '../../commands/sync';
import { useI18n } from '../../hooks/useI18n';
import { SqlPreview } from './SqlPreview';
import type { SyncState } from './utils';

export interface DataSyncPreviewStepProps {
  syncState: SyncState;
  executeProgress: string;
  sourceDbSessionId: string | undefined;
  targetDbSessionId: string | undefined;
  sourceDatabase: string;
  targetDatabase: string;
  sourceSchema: string;
  targetSchema: string;
  tables: DataSyncTableResult[];
  options: SyncOptions;
  selectedRows: DataSyncSelectedRow[];
  tableSelections: DataSyncTableSelection[];
}

/**
 * Fifth step of the Data Sync wizard: the SQL preview of the pending write,
 * with a progress overlay while the execution runs.
 */
export function DataSyncPreviewStep({
  syncState,
  executeProgress,
  sourceDbSessionId,
  targetDbSessionId,
  sourceDatabase,
  targetDatabase,
  sourceSchema,
  targetSchema,
  tables,
  options,
  selectedRows,
  tableSelections,
}: DataSyncPreviewStepProps) {
  const { t } = useI18n();

  return (
    <div className="relative flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border border-edge">
      {syncState === 'executing' && (
        <div
          data-testid="data-sync-executing-overlay"
          className="absolute inset-0 z-10 flex flex-col items-center justify-center gap-2 bg-surface/90 text-sm text-fg-muted"
        >
          <Spinner size="2xl" tone="accent" />
          <span>{executeProgress || t('sync.executing')}</span>
        </div>
      )}
      {sourceDbSessionId && targetDbSessionId ? (
        <SqlPreview
          sourceConnId={sourceDbSessionId}
          targetConnId={targetDbSessionId}
          sourceDatabase={sourceDatabase}
          targetDatabase={targetDatabase}
          sourceSchema={sourceSchema}
          targetSchema={targetSchema}
          tables={tables}
          options={options}
          selectedRows={selectedRows}
          tableSelections={tableSelections}
        />
      ) : (
        <div
          data-testid="data-sync-preview-session-required"
          className="flex flex-1 items-center justify-center text-sm text-fg-muted"
        >
          {t('sync.sessionRequired')}
        </div>
      )}
    </div>
  );
}
