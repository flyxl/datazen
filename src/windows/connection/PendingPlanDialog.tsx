import { useI18n } from '../../hooks/useI18n';
import { Button } from '../../components/ui/Button';
import { Dialog } from '../../components/ui/Dialog';
import type { RowChangePlan } from '../../lib/tableChanges';

/**
 * Preview of the plan a commit would execute.
 *
 * The statements are listed in the order they would run — inserts, then
 * updates, then deletes — so the preview reads like the transaction does. The
 * SQL is a **preview**: it is not the statement that gets executed (the host
 * rebuilds the bound statement from the plan metadata before it runs).
 */
export function PendingPlanDialog({
  plan,
  open,
  onClose,
}: {
  plan: RowChangePlan | null;
  open: boolean;
  onClose: () => void;
}) {
  const { t } = useI18n();
  if (!plan) return null;
  const inserts = plan.inserts ?? [];
  const statements = [...inserts, ...plan.updates, ...plan.deletes];
  /**
   * The counts are rendered **once**: the wording already carries the number
   * (`{count} insert(s)`), so prefixing the raw count would read "1 1
   * insert(s)".
   */
  const description = [
    plan.table.table,
    ...(inserts.length > 0 ? [t('tableData.pendingInserts', { count: inserts.length })] : []),
    t('tableData.pendingUpdates', { count: plan.updates.length }),
    t('tableData.pendingDeletes', { count: plan.deletes.length }),
  ].join(' · ');
  return (
    <Dialog
      open={open}
      title={t('tableData.previewTitle')}
      description={description}
      onClose={onClose}
      testId="pending-plan-dialog"
      footer={
        <Button size="sm" variant="secondary" onClick={onClose}>
          {t('common.close')}
        </Button>
      }
    >
      <div className="space-y-3 text-xs">
        <div>
          <div className="font-medium text-fg">{t('tableData.previewFingerprint')}</div>
          <code className="mt-1 block break-all rounded bg-surface px-2 py-1 font-mono text-fg-muted">
            {plan.fingerprint}
          </code>
        </div>
        <div>
          <div className="font-medium text-fg">{t('tableData.previewSql')}</div>
          <div className="mt-1 space-y-2">
            {statements.length === 0 ? (
              <div className="text-fg-muted">{t('tableData.noPendingChanges')}</div>
            ) : (
              statements.map((statement, index) => (
                <div
                  key={`${statement.sqlTemplate}-${index}`}
                  className="rounded border border-edge bg-surface p-2"
                >
                  <code className="block whitespace-pre-wrap break-words font-mono text-fg-secondary">
                    {statement.sqlTemplate}
                  </code>
                  {statement.parameterSummary.length > 0 && (
                    <div className="mt-1 text-fg-muted">
                      {t('tableData.previewParameters')}: {statement.parameterSummary.join(', ')}
                    </div>
                  )}
                </div>
              ))
            )}
          </div>
        </div>
        {plan.warnings.length > 0 && (
          <div className="rounded border border-amber-500/30 bg-amber-500/10 p-2 text-amber-200">
            <div className="font-medium">{t('tableData.previewWarnings')}</div>
            <ul className="mt-1 list-disc pl-4">
              {plan.warnings.map((warning) => (
                <li key={`${warning.code}-${warning.message}`}>{warning.message}</li>
              ))}
            </ul>
          </div>
        )}
      </div>
    </Dialog>
  );
}
