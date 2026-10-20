import { Button } from '../../components/ui/Button';
import type { DataSyncJobDetails } from '../../commands/sync';
import { useI18n } from '../../hooks/useI18n';

export interface DataSyncJobStatusPanelProps {
  /** Jobs whose write outcome still needs a human decision. */
  pendingVerificationCount: number;
  /** Error of the job hydration, shown as an amber hint. */
  hydrationError: string | null;
  /** Durable receipts of the jobs that never ran. */
  notExecutedCount: number;
  durableJobs: DataSyncJobDetails[];
  verifyingRecoveryJob: string | null;
  cancellingDurableJob: string | null;
  onVerifyRecovery: (details: DataSyncJobDetails) => void;
  onCancelJob: (jobId: string) => void;
}

/**
 * Status hints of the Data Sync window: pending verification, failed
 * hydration, restart receipts and the list of the durable job receipts.
 */
export function DataSyncJobStatusPanel({
  pendingVerificationCount,
  hydrationError,
  notExecutedCount,
  durableJobs,
  verifyingRecoveryJob,
  cancellingDurableJob,
  onVerifyRecovery,
  onCancelJob,
}: DataSyncJobStatusPanelProps) {
  const { t } = useI18n();

  return (
    <>
      {pendingVerificationCount ? (
        <div className="border-b border-edge px-6 py-2 text-xs text-amber-400">
          {t('migrationJob.pendingVerificationHint')}
        </div>
      ) : null}
      {hydrationError ? (
        <div className="border-b border-edge px-6 py-2 text-xs text-amber-400">
          {hydrationError}
        </div>
      ) : null}
      {notExecutedCount ? (
        <div className="border-b border-edge px-6 py-2 text-xs text-amber-400">
          {t('migrationJob.notExecutedHint')}
        </div>
      ) : null}
      {durableJobs.length ? (
        <div
          data-testid="data-sync-job-history"
          className="max-h-24 overflow-auto border-b border-edge px-6 py-2 text-xs text-fg-muted"
        >
          {durableJobs.slice(0, 5).map((details) => {
            const { job, recovery, domainResults } = details;
            const committed = Number(job.progress.committed);
            const stateLabel =
              recovery?.verdict === 'notExecuted'
                ? t('migrationJob.notExecutedJob')
                : recovery?.verdict === 'pendingVerification'
                  ? t('migrationJob.pendingVerificationHint')
                  : job.state;
            const counters = domainResults.flatMap((result) => result.counters);
            const committedResult = Number(
              counters.find((counter) => counter.code === 'committed')?.value ?? committed,
            );
            return (
              <div key={job.jobId} className="flex items-center justify-between gap-3 py-0.5">
                <span>{job.kind}</span>
                <span>{stateLabel}</span>
                <span>
                  {t('migration.verdict.committedRows')}: {committedResult}
                </span>
                {recovery?.verdict === 'pendingVerification' ? (
                  <Button
                    variant="ghost"
                    size="sm"
                    disabled={verifyingRecoveryJob === job.jobId}
                    onClick={() => onVerifyRecovery(details)}
                    data-testid={`data-sync-verify-recovery-${job.jobId}`}
                  >
                    {verifyingRecoveryJob === job.jobId
                      ? t('migrationJob.verifyingRecovery')
                      : t('migrationJob.verifyRecovery')}
                  </Button>
                ) : null}
                {job.state === 'queued' || job.state === 'running' ? (
                  <Button
                    variant="ghost"
                    size="sm"
                    disabled={job.cancelRequested || cancellingDurableJob === job.jobId}
                    onClick={() => onCancelJob(job.jobId)}
                    data-testid={`data-sync-cancel-job-${job.jobId}`}
                  >
                    {job.cancelRequested || cancellingDurableJob === job.jobId
                      ? t('migrationJob.cancellingJob')
                      : t('migrationJob.cancelJob')}
                  </Button>
                ) : null}
              </div>
            );
          })}
        </div>
      ) : null}
    </>
  );
}
