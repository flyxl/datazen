import { Spinner } from '../../components/ui/Spinner';
import type { DataSyncSourceFilter } from '../../commands/sync';
import { useI18n } from '../../hooks/useI18n';
import { MappingPanel } from './MappingPanel';
import type { DataSyncTableResult } from './mappingView';
import type { SyncState } from './utils';

export interface DataSyncObjectsStepProps {
  syncState: SyncState;
  rows: DataSyncTableResult[];
  disabledTables: Set<string>;
  onToggleDisabled: (sourceTable: string) => void;
  onUpdateSourceFilter: (sourceTable: string, sourceFilter: DataSyncSourceFilter | undefined) => void;
  onOpenSchemaDiff: () => void;
  onOpenDataTransfer: () => void;
}

/**
 * Third step of the Data Sync wizard: the inspected table mapping the user
 * reviews before the comparison starts.
 */
export function DataSyncObjectsStep({
  syncState,
  rows,
  disabledTables,
  onToggleDisabled,
  onUpdateSourceFilter,
  onOpenSchemaDiff,
  onOpenDataTransfer,
}: DataSyncObjectsStepProps) {
  const { t } = useI18n();

  return (
    <div data-testid="data-sync-objects-step" className="space-y-3">
      {syncState === 'inspecting' ? (
        <div className="flex justify-center py-12 text-sm text-fg-muted">
          <Spinner size="xl" tone="accent" />
          {t('sync.inspecting')}
        </div>
      ) : rows.length > 0 ? (
        <MappingPanel
          rows={rows}
          disabledTables={disabledTables}
          compared={false}
          onToggleDisabled={onToggleDisabled}
          onOpenSchemaDiff={onOpenSchemaDiff}
          onOpenDataTransfer={onOpenDataTransfer}
          onUpdateSourceFilter={onUpdateSourceFilter}
        />
      ) : (
        <div className="rounded-lg border border-edge bg-surface-alt px-4 py-8 text-center text-sm text-fg-muted">
          {t('sync.noTablesFound')}
        </div>
      )}
    </div>
  );
}
