import { Loader2, Square } from 'lucide-react';
import { useI18n } from '../../hooks/useI18n';
import { tid } from '../../lib/tid';
import type { QueryExecutionViewModel } from '../../lib/queryExecutionViewModel';

export interface QueryExecutionStatusProps {
  viewModel: QueryExecutionViewModel;
  onCancel?: () => void;
  showCancel?: boolean;
}

export function QueryExecutionStatus({ viewModel, onCancel, showCancel = true }: QueryExecutionStatusProps) {
  const { t } = useI18n();
  if (viewModel.phase === 'idle') return null;
  const active = viewModel.phase === 'running' || viewModel.phase === 'cancel_requested';
  const canCancel = active && viewModel.cancelState === 'available';
  const phaseLabel =
    viewModel.phase === 'cancel_requested'
      ? t('query.cancelling')
      : viewModel.phase === 'running'
        ? t('query.running')
        : viewModel.phase === 'cancelled'
          ? t('query.cancelled')
          : viewModel.phase === 'failed' || viewModel.phase === 'outcome_unknown'
            ? t('common.failed')
            : t('common.success');

  return (
    <div
      className="flex shrink-0 items-center gap-2 text-[11px] text-fg-muted"
      role="status"
      aria-live="polite"
      {...tid('query-execution-status')}
    >
      {active && <Loader2 className="h-3 w-3 animate-spin" />}
      <span>{phaseLabel}</span>
      {viewModel.elapsedMs != null && <span>{viewModel.elapsedMs} ms</span>}
      {viewModel.rowCount != null && <span>{viewModel.rowCount} {t('common.rows')}</span>}
      {viewModel.affectedRows != null && (
        <span>{viewModel.affectedRows} {t('query.affectedRows')}</span>
      )}
      {active && showCancel && (
        <button
          type="button"
          className="inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-red-300 hover:bg-red-500/10 disabled:cursor-not-allowed disabled:opacity-50"
          disabled={!canCancel}
          onClick={onCancel}
          title={
            viewModel.cancelCapability === 'unsupported'
              ? t('query.cancelUnavailable')
              : viewModel.cancelCapability === 'unknown'
                ? t('query.cancelUnknown')
                : undefined
          }
          {...tid('editor-stop-button')}
        >
          <Square className="h-3 w-3" />
          {viewModel.phase === 'cancel_requested' ? t('query.cancelling') : t('query.stop')}
        </button>
      )}
      {active && viewModel.cancelCapability !== 'supported' && (
        <span className="shrink-0 text-[11px] text-fg-muted">
          {viewModel.cancelCapability === 'unknown'
            ? t('query.cancelUnknown')
            : t('query.cancelUnavailable')}
        </span>
      )}
    </div>
  );
}
