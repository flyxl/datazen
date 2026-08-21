import { useI18n } from '../../hooks/useI18n';
import { Button } from '../../components/ui/Button';
import type { CompareSummaryStats } from './mappingView';

interface CompareSummaryProps {
  stats: CompareSummaryStats;
  onExplainDiff?: () => void;
  onCopyReport?: () => void;
  explainLoading?: boolean;
}

export function CompareSummary({
  stats,
  onExplainDiff,
  onCopyReport,
  explainLoading = false,
}: CompareSummaryProps) {
  const { t } = useI18n();

  return (
    <div
      data-testid="data-sync-summary"
      className="flex shrink-0 flex-wrap items-center gap-x-4 gap-y-1 border-b border-edge bg-surface-alt/30 px-6 py-2 text-xs"
    >
      <span className="font-semibold uppercase tracking-wider text-fg-muted">
        {t('sync.compareSummaryTitle')}
      </span>
      <span className="tabular-nums text-green-600 dark:text-green-400">
        {t('sync.summaryInserts', { count: stats.inserts })}
      </span>
      <span className="tabular-nums text-blue-600 dark:text-blue-400">
        {t('sync.summaryUpdates', { count: stats.updates })}
      </span>
      <span className="tabular-nums text-red-600 dark:text-red-400">
        {t('sync.summaryDeletes', { count: stats.deletes })}
      </span>
      <span className="text-fg-muted">
        {t('sync.summaryUnchanged', { count: stats.unchangedTables })}
      </span>
      {stats.incompatible > 0 && (
        <span className="text-amber-600 dark:text-amber-400">
          {t('sync.summaryIncompatible', { count: stats.incompatible })}
        </span>
      )}
      {(onExplainDiff || onCopyReport) && (
        <span className="ml-auto flex items-center gap-2">
          {onCopyReport && (
            <Button
              variant="ghost"
              size="sm"
              data-testid="data-sync-copy-report"
              onClick={onCopyReport}
            >
              {t('sync.copyReport')}
            </Button>
          )}
          {onExplainDiff && (
            <Button
              variant="secondary"
              size="sm"
              data-testid="data-sync-explain-diff"
              disabled={explainLoading}
              onClick={onExplainDiff}
            >
              {t('sync.explainDiff')}
            </Button>
          )}
        </span>
      )}
    </div>
  );
}
