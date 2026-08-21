import { Loader2, Play } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { useI18n } from '../../hooks/useI18n';

interface ExecuteBarProps {
  selectedRows: number;
  hasDeletes: boolean;
  targetReadOnly: boolean;
  executing: boolean;
  canExecute: boolean;
  onExecute: () => void;
  onCancel: () => void;
}

export function ExecuteBar({
  selectedRows,
  hasDeletes,
  targetReadOnly,
  executing,
  canExecute,
  onExecute,
  onCancel,
}: ExecuteBarProps) {
  const { t } = useI18n();

  const enabled = canExecute && !targetReadOnly && selectedRows > 0;

  return (
    <div
      className="flex shrink-0 items-center gap-3 border-t border-edge px-6 py-3"
      data-testid={enabled ? 'data-sync-execute' : undefined}
    >
      <span className="text-xs text-fg-muted">
        {t('sync.selectedRows', { count: selectedRows })}
      </span>
      {targetReadOnly && (
        <span className="text-xs text-amber-600 dark:text-amber-400">
          {t('sync.targetReadOnly')}
        </span>
      )}
      {hasDeletes && !targetReadOnly && (
        <span className="text-xs text-red-600 dark:text-red-400">{t('sync.deleteWillApply')}</span>
      )}
      <div className="flex-1" />
      {executing && (
        <Button variant="ghost" data-testid="data-sync-cancel" onClick={onCancel}>
          {t('common.cancel')}
        </Button>
      )}
      <Button
        variant="primary"
        onClick={onExecute}
        disabled={!enabled || executing}
        title={
          targetReadOnly
            ? t('sync.targetReadOnly')
            : !enabled
              ? t('sync.executeUnavailable')
              : undefined
        }
        data-testid={enabled ? 'data-sync-start' : 'data-sync-start-disabled'}
      >
        {executing ? <Loader2 className="h-4 w-4 animate-spin" /> : <Play className="h-4 w-4" />}
        {t('sync.execute')}
      </Button>
    </div>
  );
}
