import { ExternalLink } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';
import {
  displayTableName,
  mappingLabelKey,
  rowDiffCounts,
  summarizeMappings,
  tableHasRowDiffs,
  tableKey,
  type DataSyncTableResult,
} from './mappingView';

interface MappingPanelProps {
  rows: DataSyncTableResult[];
  disabledTables: Set<string>;
  compared: boolean;
  onToggleDisabled: (sourceTable: string) => void;
  onOpenSchemaDiff: () => void;
  onOpenDataTransfer?: () => void;
}

export function MappingPanel({
  rows,
  disabledTables,
  compared,
  onToggleDisabled,
  onOpenSchemaDiff,
  onOpenDataTransfer,
}: MappingPanelProps) {
  const { t } = useI18n();
  const summary = summarizeMappings(rows);

  if (rows.length === 0) return null;

  return (
    <div className="shrink-0 border-b border-edge px-4 py-2">
      <div className="mb-2 flex items-center gap-2">
        <span className="text-xs font-semibold uppercase tracking-wider text-fg-muted">
          {compared ? t('sync.mappingAfterCompare') : t('sync.mappingTitle')}
        </span>
        <div className="flex-1" />
        <span data-testid="data-sync-mapping-summary" className="text-xs text-fg-muted">
          {t('sync.mappingSummary', summary)}
        </span>
      </div>

      <div className="max-h-36 overflow-auto rounded-lg border border-edge">
        <div className="flex items-center gap-3 bg-surface-alt px-3 py-1.5 text-[11px] font-semibold uppercase tracking-wider text-fg-muted">
          <div className="w-6" />
          <div className="min-w-0 flex-1">{t('sync.tableName')}</div>
          <div className="w-40 text-right">{t('sync.status')}</div>
          <div className="w-24" />
        </div>
        {rows.map((row) => {
          const name = displayTableName(row);
          const isDisabled = row.status === 'DISABLED' || disabledTables.has(row.sourceTable);
          const canToggle = row.status === 'MATCHED' || row.status === 'DISABLED';
          return (
            <div
              key={tableKey(row)}
              data-testid="data-sync-mapping-row"
              className={cn(
                'border-t border-edge px-3 py-1.5',
                row.status === 'INCOMPATIBLE' && 'bg-amber-500/5',
                isDisabled && 'opacity-60',
              )}
            >
              <div className="flex items-center gap-3 text-sm">
                <input
                  type="checkbox"
                  className="h-3.5 w-3.5"
                  checked={!isDisabled}
                  disabled={!canToggle}
                  title={canToggle ? t('sync.includeTable') : undefined}
                  onChange={() => {
                    if (canToggle && row.sourceTable) onToggleDisabled(row.sourceTable);
                  }}
                />
                <div className="min-w-0 flex-1 truncate font-mono text-xs">{name}</div>
                <div className="w-40 text-right text-xs text-fg-secondary">
                  {row.status === 'MATCHED' && compared && tableHasRowDiffs(row)
                    ? t('sync.rowDiffs', rowDiffCounts(row))
                    : t(mappingLabelKey(row.status))}
                </div>
                <div className="flex w-24 justify-end gap-1">
                  {row.status === 'INCOMPATIBLE' && (
                    <>
                      <Button
                        variant="ghost"
                        size="sm"
                        className="text-[10px]"
                        onClick={onOpenSchemaDiff}
                      >
                        {t('common.schemaDiff')}
                      </Button>
                      {onOpenDataTransfer && (
                        <Button
                          variant="ghost"
                          size="sm"
                          className="text-[10px]"
                          data-testid="data-sync-open-transfer"
                          onClick={onOpenDataTransfer}
                        >
                          {t('common.dataTransfer')}
                        </Button>
                      )}
                    </>
                  )}
                </div>
              </div>
              {row.incompatibleReason && (
                <div
                  className="ml-9 truncate text-[11px] text-amber-600 dark:text-amber-400"
                  title={row.incompatibleReason}
                >
                  {row.incompatibleReason}
                </div>
              )}
            </div>
          );
        })}
      </div>

      {summary.incompatible > 0 && (
        <p className="mt-2 flex items-center gap-1 text-xs text-fg-secondary">
          <ExternalLink className="h-3 w-3 shrink-0" />
          {t('sync.incompatibleHint')}
        </p>
      )}
    </div>
  );
}
