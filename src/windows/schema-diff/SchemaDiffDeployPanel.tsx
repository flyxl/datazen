import type { SchemaDiffDeployResult, SchemaDiffPlan } from '../../commands/schemaDiff';
import {
  DESTRUCTIVE_CONFIRM_TOKEN,
  dialectSupportsTransactionalDdl,
  planHasDestructive,
} from '../../commands/schemaDiff';
import { useI18n } from '../../hooks/useI18n';
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
  onDeploy: () => void;
  result: SchemaDiffDeployResult | null;
}) {
  const { t } = useI18n();
  const hasDestructive = planHasDestructive(plan);
  const txSupported = dialectSupportsTransactionalDdl(plan.targetDialect);
  const canRun = canRunDeploy({
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
        <input
          type="checkbox"
          checked={useTransaction && txSupported}
          disabled={!txSupported}
          onChange={(e) => onUseTransactionChange(e.target.checked)}
        />
        {t('schemaDiff.useTransaction')}
        {!txSupported && (
          <span className="text-xs text-fg-muted">({t('schemaDiff.txUnsupported')})</span>
        )}
      </label>

      <label className="flex items-center gap-2">
        <input
          type="checkbox"
          checked={requireRollback}
          onChange={(e) => onRequireRollbackChange(e.target.checked)}
        />
        {t('schemaDiff.requireRollback')}
      </label>

      {requireRollback && !plan.rollbackCompleteness.complete && (
        <p className="text-xs text-danger">
          {t('schemaDiff.rollbackIncomplete')}: {plan.rollbackCompleteness.missing.join('; ')}
        </p>
      )}

      {hasDestructive && (
        <label className="block space-y-1">
          <span className="text-fg-secondary">
            {t('schemaDiff.confirmDeploy', { token: DESTRUCTIVE_CONFIRM_TOKEN })}
          </span>
          <input
            className="w-full rounded-md border border-edge bg-surface px-3 py-2 font-mono text-sm"
            value={confirmText}
            onChange={(e) => onConfirmTextChange(e.target.value)}
            placeholder={DESTRUCTIVE_CONFIRM_TOKEN}
          />
        </label>
      )}

      <button
        type="button"
        className="rounded-md bg-accent px-3 py-2 text-sm text-accent-fg disabled:opacity-50"
        disabled={!canRun || deploying}
        onClick={onDeploy}
        data-testid="schema-diff-deploy"
      >
        {deploying ? t('schemaDiff.deploying') : t('schemaDiff.deploy')}
      </button>

      {result && (
        <div className="rounded border border-edge bg-surface-alt p-3 text-xs">
          <div className="font-medium text-fg">
            {t('schemaDiff.deployStatus')}: {result.status}
          </div>
          <div className="text-fg-secondary">
            {result.executedCount}/{result.statementCount} {t('schemaDiff.executed')}
          </div>
          {result.errors.length > 0 && (
            <ul className="mt-2 list-inside list-disc text-danger">
              {result.errors.map((e) => (
                <li key={e}>{e}</li>
              ))}
            </ul>
          )}
        </div>
      )}
    </div>
  );
}
