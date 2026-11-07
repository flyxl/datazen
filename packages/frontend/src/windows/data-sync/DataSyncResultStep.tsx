import { Button } from '../../components/ui/Button';
import type { DataSyncExecutionResult } from '../../commands/sync';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';
import { CompareSummary } from './CompareSummary';
import type { CompareSummaryStats } from './mappingView';

export interface DataSyncResultStepProps {
  lastExecutionResult: DataSyncExecutionResult | null;
  stats: CompareSummaryStats;
  onCopyReport: () => void;
  onExplainDiff: () => void;
  explainLoading: boolean;
  onReCompare: () => void;
}

/**
 * Last step of the Data Sync wizard: the outcome of the execution and the
 * comparison that follows it.
 */
export function DataSyncResultStep({
  lastExecutionResult,
  stats,
  onCopyReport,
  onExplainDiff,
  explainLoading,
  onReCompare,
}: DataSyncResultStepProps) {
  const { t } = useI18n();
  const outcome =
    lastExecutionResult?.outcome ??
    (lastExecutionResult?.rolledBack ? 'rolled_back' : 'committed');
  const isUnknown = outcome === 'unknown';
  const isPartial = outcome === 'partially_applied';
  const wasRolledBack = outcome === 'rolled_back';

  return (
    <div data-testid="data-sync-result" className="space-y-4 rounded-lg border border-edge bg-surface-alt p-6">
      <div
        data-testid="data-sync-execute-done"
        className={cn(
          'flex flex-wrap items-center gap-3 text-sm',
          isUnknown || wasRolledBack || isPartial
            ? 'text-amber-700 dark:text-amber-400'
            : 'text-green-700 dark:text-green-400',
        )}
        role="status"
      >
        <span>
          {isUnknown
            ? t('sync.executionUnknown')
            : isPartial
              ? t('migration.verdict.partial')
              : wasRolledBack
                ? lastExecutionResult?.rollbackReason || t('sync.rolledBack')
                : t('sync.executeDone')}
        </span>
        {lastExecutionResult?.skipped ? (
          <span className="text-amber-600 dark:text-amber-400">
            {t('sync.conflictsSkipped', { count: lastExecutionResult.skipped })}
          </span>
        ) : null}
        <Button
          variant="secondary"
          size="sm"
          data-testid="data-sync-re-compare"
          onClick={onReCompare}
        >
          {t('sync.reCompare')}
        </Button>
      </div>
      <CompareSummary
        stats={stats}
        onCopyReport={onCopyReport}
        onExplainDiff={onExplainDiff}
        explainLoading={explainLoading}
      />
    </div>
  );
}
