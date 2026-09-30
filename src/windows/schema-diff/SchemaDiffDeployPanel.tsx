import type { SchemaDiffDeployResult, SchemaDiffPlan } from '../../commands/schemaDiff';
import {
  DESTRUCTIVE_CONFIRM_TOKEN,
  dialectSupportsTransactionalDdl,
  planHasDestructive,
} from '../../commands/schemaDiff';
import { useI18n } from '../../hooks/useI18n';
import { Button } from '../../components/ui/Button';
import { Checkbox } from '../../components/ui/Checkbox';
import { CopyableError } from '../../components/ui/CopyableError';
import { ErrorBanner } from '../../components/ui/ErrorBanner';
import { Input } from '../../components/ui/Input';
import { canRunDeploy } from '../../lib/schemaDiffConfirm';

export function SchemaDiffDeployPanel({
  plan,
  targetLabel,
  useTransaction,
  onUseTransactionChange,
  requireRollback,
  onRequireRollbackChange,
  confirmText,
  onConfirmTextChange,
  deploying,
  hideDeployButton = false,
  onDeploy,
  result,
}: {
  plan: SchemaDiffPlan;
  targetLabel: string;
  useTransaction: boolean;
  onUseTransactionChange: (v: boolean) => void;
  requireRollback: boolean;
  onRequireRollbackChange: (v: boolean) => void;
  confirmText: string;
  onConfirmTextChange: (v: string) => void;
  deploying: boolean;
  hideDeployButton?: boolean;
  onDeploy: () => void;
  result: SchemaDiffDeployResult | null;
}) {
  const { t } = useI18n();
  const hasDestructive = planHasDestructive(plan);
  const txSupported = dialectSupportsTransactionalDdl(plan.targetDialect);
  const transactionRequired = plan.statements.some((statement) => statement.requiresTransaction);
  const transactionEnabled = transactionRequired || (useTransaction && txSupported);
  const canRun =
    !result &&
    !plan.requirements?.length &&
    (!transactionRequired || (txSupported && transactionEnabled)) &&
    (!requireRollback || (txSupported && transactionEnabled)) &&
    canRunDeploy({
      hasDestructive,
      confirmText,
      requireRollback,
      rollbackComplete: plan.rollbackCompleteness.complete,
      statementCount: plan.statements.length,
    });

  return (
    <div className="space-y-3 text-sm">
      <div className="rounded border border-edge bg-surface-alt p-3 text-xs">
        <div>
          {t('schemaDiff.reviewTarget')}: <span className="font-mono text-fg">{targetLabel}</span>
        </div>
        <div>
          {t('schemaDiff.reviewTables')}:{' '}
          <span className="font-mono text-fg">{plan.tables.join(', ')}</span>
        </div>
        <div>
          {t('schemaDiff.statements')}: {plan.statements.length}
        </div>
      </div>

      <label className="flex items-center gap-2">
        <Checkbox
          checked={transactionRequired || (useTransaction && txSupported)}
          disabled={!txSupported || transactionRequired}
          onChange={(e) => onUseTransactionChange(e.target.checked)}
        />
        {t('schemaDiff.useTransaction')}
        {transactionRequired && (
          <span className="text-xs text-fg-muted">({t('schemaDiff.transactionRequired')})</span>
        )}
        {!txSupported && (
          <span className="text-xs text-fg-muted">({t('schemaDiff.txUnsupported')})</span>
        )}
      </label>

      <label className="flex items-center gap-2">
        <Checkbox
          checked={requireRollback}
          onChange={(e) => onRequireRollbackChange(e.target.checked)}
        />
        {t('schemaDiff.requireRollback')}
      </label>

      {requireRollback && !plan.rollbackCompleteness.complete && (
        <ErrorBanner as="p" className="select-text">
          {t('schemaDiff.rollbackIncomplete')}: {plan.rollbackCompleteness.missing.join('; ')}
        </ErrorBanner>
      )}

      {hasDestructive && (
        <label className="block space-y-1">
          <span className="text-fg-secondary">
            {t('schemaDiff.confirmDeploy', { token: DESTRUCTIVE_CONFIRM_TOKEN })}
          </span>
          <Input
            value={confirmText}
            onChange={(e) => onConfirmTextChange(e.target.value)}
            placeholder={DESTRUCTIVE_CONFIRM_TOKEN}
          />
        </label>
      )}

      {!hideDeployButton && (
        <Button
          variant="run"
          size="md"
          disabled={!canRun || deploying}
          onClick={onDeploy}
          data-testid="schema-diff-deploy"
        >
          {deploying ? t('schemaDiff.deploying') : t('schemaDiff.deploy')}
        </Button>
      )}

      {result && (
        <div
          className="rounded border border-edge bg-surface-alt p-3 text-xs"
          data-testid="schema-diff-deploy-result"
        >
          <div className="font-medium text-fg" data-testid="schema-diff-deploy-status">
            {t('schemaDiff.deployStatus')}: {result.status}
          </div>
          <div className="text-fg-secondary" data-testid="schema-diff-deploy-count">
            {result.executedCount}/{result.statementCount} {t('schemaDiff.executed')}
          </div>
          {result.errors.length > 0 && (
            <ul className="mt-2 list-inside list-disc" data-testid="schema-diff-deploy-errors">
              {result.errors.map((e) => (
                <li key={e}>
                  <CopyableError message={e} className="error-message text-xs" />
                </li>
              ))}
            </ul>
          )}
        </div>
      )}
    </div>
  );
}
