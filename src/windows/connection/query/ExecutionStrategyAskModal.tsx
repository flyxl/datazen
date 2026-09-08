import { Dialog } from '../../../components/ui/Dialog';
import { Button } from '../../../components/ui/Button';
import { useI18n } from '../../../hooks/useI18n';
import type { StatementTargetInfo } from './resolveExecutionTarget';

export interface ExecutionStrategyAskModalProps {
  open: boolean;
  currentStatement?: StatementTargetInfo;
  statementCount: number;
  entireScript: string;
  onExecuteCurrent: () => void;
  onExecuteEntire: () => void;
  onCancel: () => void;
}

export function ExecutionStrategyAskModal({
  open,
  currentStatement,
  statementCount,
  entireScript,
  onExecuteCurrent,
  onExecuteEntire,
  onCancel,
}: ExecutionStrategyAskModalProps) {
  const { t } = useI18n();

  if (!open) return null;

  const currentPreview = currentStatement?.sql
    ? currentStatement.sql.length > 120
      ? `${currentStatement.sql.slice(0, 120)}…`
      : currentStatement.sql
    : '';

  const entirePreview = entireScript.length > 120 ? `${entireScript.slice(0, 120)}…` : entireScript;

  return (
    <Dialog
      open={open}
      onClose={onCancel}
      title={t('query.executionStrategy.askTitle')}
      className="max-w-md"
    >
      <div className="space-y-4 py-2">
        <p className="text-xs text-fg-muted">{t('query.executionStrategy.askDesc')}</p>

        {currentStatement && (
          <div
            className="group flex cursor-pointer flex-col gap-1.5 rounded-lg border border-edge bg-surface p-3 transition hover:border-accent hover:bg-surface-raised"
            onClick={onExecuteCurrent}
            role="button"
            tabIndex={0}
            onKeyDown={(e) => {
              if (e.key === 'Enter' || e.key === ' ') onExecuteCurrent();
            }}
          >
            <div className="flex items-center justify-between text-xs font-medium text-fg">
              <span>
                {t('query.executionStrategy.runCurrent', {
                  from: currentStatement.fromLine,
                  to: currentStatement.toLine,
                })}
              </span>
            </div>
            <pre className="max-h-20 overflow-hidden font-mono text-[11px] text-fg-muted">
              {currentPreview}
            </pre>
          </div>
        )}

        <div
          className="group flex cursor-pointer flex-col gap-1.5 rounded-lg border border-edge bg-surface p-3 transition hover:border-accent hover:bg-surface-raised"
          onClick={onExecuteEntire}
          role="button"
          tabIndex={0}
          onKeyDown={(e) => {
            if (e.key === 'Enter' || e.key === ' ') onExecuteEntire();
          }}
        >
          <div className="flex items-center justify-between text-xs font-medium text-fg">
            <span>{t('query.executionStrategy.runEntire', { count: statementCount })}</span>
          </div>
          <pre className="max-h-20 overflow-hidden font-mono text-[11px] text-fg-muted">
            {entirePreview}
          </pre>
        </div>

        <div className="flex justify-end pt-2">
          <Button variant="ghost" size="sm" onClick={onCancel}>
            {t('common.cancel')}
          </Button>
        </div>
      </div>
    </Dialog>
  );
}
