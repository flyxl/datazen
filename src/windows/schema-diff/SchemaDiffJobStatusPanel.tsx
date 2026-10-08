import type { SchemaDiffJobDetails } from '../../commands/schemaDiff';
import { useI18n } from '../../hooks/useI18n';
import { readJobCounter } from '../../lib/migrationJobVerdict';
import { Button } from '../../components/ui/Button';
import type { SchemaDiffTrackedJob } from './useSchemaDiffJobLifecycle';

const STATE_KEYS: Record<string, string> = {
  queued: 'schemaDiff.jobQueued',
  running: 'schemaDiff.jobRunning',
  succeeded: 'schemaDiff.jobSucceeded',
  failed: 'schemaDiff.jobFailed',
  cancelled: 'schemaDiff.jobCancelled',
};

export function SchemaDiffJobStatusPanel({
  currentJob,
  latestApply,
  cancelOutcome,
  verifying,
  targetReady,
  onCancel,
  onVerify,
}: {
  currentJob: SchemaDiffTrackedJob | null;
  latestApply: SchemaDiffJobDetails | null;
  cancelOutcome: 'idle' | 'requested' | 'unavailable';
  verifying: boolean;
  targetReady: boolean;
  onCancel: () => void;
  onVerify: (jobId: string) => void;
}) {
  const { t } = useI18n();
  const details = currentJob?.details;
  const view = details?.details.job;
  const state = view?.state ?? currentJob?.state;
  const kind = view?.kind ?? currentJob?.kind;
  const latestApplyJob = latestApply?.details.job;
  const latestApplyRecovery = latestApply?.details.recovery;
  const recoveryDetails =
    view?.pendingVerificationReason != null
      ? details
      : latestApplyJob?.pendingVerificationReason != null ||
          latestApplyRecovery?.verdict === 'notExecuted' ||
          latestApplyRecovery?.verdict === 'requireManualReview'
        ? latestApply
        : details;
  const recoveryView = recoveryDetails?.details.job;
  const recovery = recoveryDetails?.details.recovery;
  const pendingVerification = recoveryView?.pendingVerificationReason != null;
  const notExecuted = recovery?.verdict === 'notExecuted';
  const notDispatchedAfterRestart =
    notExecuted && recovery?.reasonCode === 'notDispatchedAfterRestart';
  const active = state === 'queued' || state === 'running';
  const hasRecoveryTargets = (recoveryDetails?.details.recoveryTargets.length ?? 0) > 0;
  const lastResult = latestApply?.deployResult;
  const counters = view?.progress;

  if (!currentJob && !latestApply) return null;

  return (
    <section
      className="border-b border-edge bg-surface-alt px-6 py-3 text-xs"
      data-testid="schema-diff-job-status"
      aria-live="polite"
    >
      {currentJob && state && kind && (
        <div className="space-y-2">
          <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
            <span className="font-medium text-fg">
              {kind === 'schemaDiffApply' ? t('schemaDiff.applyJob') : t('schemaDiff.prepareJob')}:{' '}
              {t(STATE_KEYS[state] ?? 'schemaDiff.jobUnknown')}
            </span>
            {view?.stage && <span className="text-fg-muted">{view.stage}</span>}
            {active && counters && (
              <span className="text-fg-secondary">
                {t('schemaDiff.jobProgress', {
                  attempted: readJobCounter(counters.attempted) ?? 0,
                  committed: readJobCounter(counters.committed) ?? 0,
                  unknown: readJobCounter(counters.unknown) ?? 0,
                })}
              </span>
            )}
            {active && (
              <Button variant="secondary" size="sm" onClick={onCancel}>
                {t('schemaDiff.requestCancel')}
              </Button>
            )}
          </div>

          {(view?.cancelRequested || cancelOutcome === 'requested') && active && (
            <p className="text-amber-400">{t('schemaDiff.cancelRequested')}</p>
          )}
          {cancelOutcome === 'unavailable' && (
            <p className="text-amber-400">{t('schemaDiff.cancelUnavailable')}</p>
          )}

          {details?.planUnavailableAfterRestart && (
            <p className="text-amber-400">{t('schemaDiff.planUnavailableAfterRestart')}</p>
          )}
        </div>
      )}

      {notDispatchedAfterRestart && (
        <p className="mt-2 text-amber-400">{t('schemaDiff.notDispatchedAfterRestart')}</p>
      )}
      {notExecuted && !notDispatchedAfterRestart && (
        <p className="mt-2 text-amber-400">{t('schemaDiff.notExecuted')}</p>
      )}

      {!notExecuted && pendingVerification && (
        <div className="mt-2 flex flex-wrap items-center gap-2 text-amber-400">
          <span>{t('schemaDiff.recoveryPending')}</span>
          {hasRecoveryTargets && recoveryView ? (
            <Button
              variant="secondary"
              size="sm"
              disabled={!targetReady || verifying}
              onClick={() => onVerify(recoveryView.jobId)}
            >
              {verifying ? t('schemaDiff.verifyingRecovery') : t('schemaDiff.verifyRecovery')}
            </Button>
          ) : (
            <span>{t('schemaDiff.recoveryIdentityUnavailable')}</span>
          )}
        </div>
      )}

      {recovery?.verdict === 'requireManualReview' && (
        <p className="mt-2 text-amber-400">
          {recovery.reasonCode === 'targetStateUnchangedReprepareRequired'
            ? t('schemaDiff.recoveryUnchanged')
            : recovery.reasonCode === 'targetStateChangedManualReview'
              ? t('schemaDiff.recoveryChanged')
              : t('schemaDiff.recoveryManualReview')}
        </p>
      )}

      {lastResult && (
        <div className="mt-2 flex flex-wrap gap-x-3 gap-y-1 border-t border-edge pt-2">
          <span className="font-medium text-fg">
            {t('schemaDiff.lastApplyReport')}: {lastResult.status}
          </span>
          <span className="text-fg-secondary">
            {lastResult.executedCount}/{lastResult.statementCount} {t('schemaDiff.executed')}
          </span>
          {latestApply?.details.recovery?.reasonCode && (
            <span className="text-amber-400">
              {t('schemaDiff.recoveryCode', { code: latestApply.details.recovery.reasonCode })}
            </span>
          )}
        </div>
      )}
    </section>
  );
}
