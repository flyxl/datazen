import { useState } from 'react';
import type { JobView } from '@datazen/backend-client';

import { transferJobCommands } from '../../commands/transferJobs';
import { Button } from '../../components/ui/Button';
import { useI18n } from '../../hooks/useI18n';
import { readJobCounter } from '../../lib/migrationJobVerdict';

const ACTIVE_STATES = new Set(['queued', 'running']);

export function DataTransferAttachedJobs({
  jobs,
  excludeJobId,
}: Readonly<{
  jobs: readonly JobView[];
  excludeJobId?: string | null;
}>) {
  const { t } = useI18n();
  const [canceling, setCanceling] = useState<ReadonlySet<string>>(() => new Set());
  const [cancelResult, setCancelResult] = useState<ReadonlyMap<string, boolean>>(
    () => new Map(),
  );
  const activeJobs = jobs.filter(
    (job) =>
      job.kind === 'dataTransferApply' &&
      ACTIVE_STATES.has(job.state) &&
      job.jobId !== excludeJobId,
  );

  if (activeJobs.length === 0) return null;

  const requestCancel = async (jobId: string) => {
    setCanceling((current) => new Set(current).add(jobId));
    try {
      const accepted = await transferJobCommands.cancel(jobId);
      setCancelResult((current) => new Map(current).set(jobId, accepted));
    } catch {
      setCancelResult((current) => new Map(current).set(jobId, false));
    } finally {
      setCanceling((current) => {
        const next = new Set(current);
        next.delete(jobId);
        return next;
      });
    }
  };

  return (
    <section
      className="space-y-2 border-b border-edge px-6 py-3"
      data-testid="data-transfer-attached-jobs"
      aria-label={t('transfer.job.attachedTitle')}
    >
      {activeJobs.map((job) => {
        const cancelSent = job.cancelRequested || cancelResult.get(job.jobId) === true;
        const cancelUnknown = cancelResult.get(job.jobId) === false;
        const counts = [
          job.progress.read,
          job.progress.converted,
          job.progress.attempted,
          job.progress.committed,
          job.progress.unknown,
        ].map((value) => readJobCounter(value) ?? 0);

        return (
          <div
            key={job.jobId}
            className="flex flex-wrap items-center gap-x-4 gap-y-2 rounded border border-edge bg-surface-alt px-3 py-2 text-xs"
            data-testid="data-transfer-attached-job"
            data-job-id={job.jobId}
            data-state={job.state}
            data-cancel-requested={String(cancelSent)}
          >
            <div className="min-w-0 flex-1">
              <p
                role="status"
                data-testid="data-transfer-attached-job-state"
                className="font-medium text-fg"
              >
                {t(`transfer.job.${job.state}`)}
                {job.stage ? ` · ${job.stage}` : ''}
              </p>
              <p data-testid="data-transfer-attached-job-progress" className="text-fg-muted">
                {t('migration.progress.counts')}: {counts.join(' / ')}
              </p>
              {cancelSent ? (
                <p data-testid="data-transfer-attached-job-cancel-requested" className="text-warning">
                  {t('migration.cancel.requestedInFlight')}
                </p>
              ) : cancelUnknown ? (
                <p data-testid="data-transfer-attached-job-cancel-unknown" className="text-warning">
                  {t('migration.cancel.unknownJob')}
                </p>
              ) : null}
            </div>
            <Button
              variant="ghost"
              data-testid="data-transfer-attached-cancel"
              data-job-id={job.jobId}
              disabled={canceling.has(job.jobId) || cancelSent}
              onClick={() => void requestCancel(job.jobId)}
            >
              {t('transfer.cancel')}
            </Button>
          </div>
        );
      })}
    </section>
  );
}
