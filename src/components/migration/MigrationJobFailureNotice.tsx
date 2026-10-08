/**
 * Fail-closed refusal notices for the P5 migration tools.
 *
 * `backendScope` and the 8 MiB `PipelineBudget` are **refusals**: the backend
 * declined to run because a rule was not satisfied. Presenting either as a
 * warning, a retry prompt or a soft failure would tell the user something the
 * backend did not agree to, which is the one thing these two rules exist to
 * prevent. A consumed `planId` is the same shape — the correct action is
 * re-review, not "try again".
 *
 * Domain-free by design: Schema Diff and Data Sync render the same four arms
 * from the same {@link TransferRunFailureKind} classification.
 */

import { Button } from '../ui/Button';
import { CopyableError } from '../ui/CopyableError';
import { useI18n } from '../../hooks/useI18n';
import type { TransferRunFailure, TransferRunFailureKind } from '../../hooks/useTransferJobRun';

const TONE_CLASS: Record<TransferRunFailureKind, string> = {
  backendScope: 'text-danger',
  pipelineBudget: 'text-warning',
  planConsumed: 'text-warning',
  stalePlan: 'text-warning',
  other: 'text-danger',
};

function titleKey(kind: TransferRunFailureKind): string {
  switch (kind) {
    case 'backendScope':
      return 'migration.failure.backendScope.title';
    case 'pipelineBudget':
      return 'migration.failure.pipelineBudget.title';
    case 'planConsumed':
      return 'migration.failure.planConsumed.title';
    case 'stalePlan':
      return 'migration.failure.stalePlan.title';
    case 'other':
      return 'migration.failure.other.title';
  }
}

function bodyKey(kind: TransferRunFailureKind): string {
  switch (kind) {
    case 'backendScope':
      return 'migration.failure.backendScope.body';
    case 'pipelineBudget':
      return 'migration.failure.pipelineBudget.body';
    case 'planConsumed':
      return 'migration.failure.planConsumed.body';
    case 'stalePlan':
      return 'migration.failure.stalePlan.body';
    case 'other':
      return 'migration.failure.other.body';
  }
}

/** Only these two kinds may offer a retry-shaped affordance, and only as re-review. */
function offersReReview(kind: TransferRunFailureKind): boolean {
  return kind === 'planConsumed' || kind === 'stalePlan';
}

export interface MigrationJobFailureNoticeProps {
  failure: TransferRunFailure;
  /** Mint a new plan. Never "apply the same plan again". */
  onReReview?: () => void;
  onDismiss?: () => void;
  testIdPrefix?: string;
  className?: string;
}

export function MigrationJobFailureNotice({
  failure,
  onReReview,
  onDismiss,
  testIdPrefix = 'migration-job',
  className,
}: MigrationJobFailureNoticeProps) {
  const { t } = useI18n();

  return (
    <div
      role="alert"
      data-testid={`${testIdPrefix}-failure`}
      data-failure-kind={failure.kind}
      data-fail-closed={failure.kind === 'backendScope' || failure.kind === 'pipelineBudget'}
      className={className ?? 'space-y-2 rounded-lg border border-edge bg-surface-alt p-4 text-sm'}
    >
      <p
        data-testid={`${testIdPrefix}-failure-title`}
        className={`text-base font-medium ${TONE_CLASS[failure.kind]}`}
      >
        {t(titleKey(failure.kind))}
      </p>
      <p data-testid={`${testIdPrefix}-failure-body`} className="text-fg-muted">
        {t(bodyKey(failure.kind))}
      </p>
      <CopyableError
        message={failure.message}
        className="error-message text-xs"
        copyButton
        data-testid={`${testIdPrefix}-failure-detail`}
      />
      {offersReReview(failure.kind) && onReReview ? (
        <Button
          variant="primary"
          data-testid={`${testIdPrefix}-re-review`}
          onClick={onReReview}
        >
          {t('migration.failure.reReview')}
        </Button>
      ) : null}
      {onDismiss ? (
        <Button variant="ghost" data-testid={`${testIdPrefix}-failure-dismiss`} onClick={onDismiss}>
          {t('common.dismiss')}
        </Button>
      ) : null}
    </div>
  );
}

export default MigrationJobFailureNotice;