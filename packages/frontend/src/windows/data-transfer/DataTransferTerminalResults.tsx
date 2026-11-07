import { useEffect, useState } from 'react';
import type { JobDetails, JobView } from '@datazen/backend-client';

import { transferJobCommands } from '../../commands/transferJobs';
import { MigrationJobVerdictPanel } from '../../components/migration/MigrationJobVerdictPanel';
import { useLocaleDomains } from '../../hooks/useLocaleDomains';
import { useI18n } from '../../hooks/useI18n';
import {
  deriveMigrationJobVerdict,
  isMigrationJobInFlight,
  readJobCounter,
} from '../../lib/migrationJobVerdict';

function latestApplyJob(jobs: readonly JobView[]): JobView | null {
  const applies = jobs.filter((job) => job.kind === 'dataTransferApply');
  if (applies.length === 0) return null;
  return applies.reduce((latest, job) => (job.createdAt > latest.createdAt ? job : latest));
}

/**
 * Restores the latest settled apply report from the durable Job details query.
 * A summary is never enough to render a completed verdict: while details load,
 * or if the query fails, this surface stays explicitly uncompleted.
 */
export function DataTransferTerminalResults({
  jobs,
  excludeJobId,
  hide,
}: Readonly<{
  jobs: readonly JobView[];
  excludeJobId?: string | null;
  hide?: boolean;
}>) {
  useLocaleDomains(['sync']);
  const { t } = useI18n();
  const latest = latestApplyJob(jobs);
  const terminalJob =
    !hide && latest && !isMigrationJobInFlight(latest.state) && latest.jobId !== excludeJobId
      ? latest
      : null;
  const [loaded, setLoaded] = useState<{ jobId: string; details: JobDetails } | null>(null);
  const [failedJobId, setFailedJobId] = useState<string | null>(null);

  useEffect(() => {
    let subscribed = true;
    setLoaded(null);
    setFailedJobId(null);
    if (!terminalJob) return () => {
      subscribed = false;
    };

    void transferJobCommands
      .getDetails(terminalJob.jobId)
      .then((details) => {
        if (!subscribed) return;
        if (details.job.jobId !== terminalJob.jobId || isMigrationJobInFlight(details.job.state)) {
          setFailedJobId(terminalJob.jobId);
          return;
        }
        setLoaded({ jobId: terminalJob.jobId, details });
      })
      .catch(() => {
        if (subscribed) setFailedJobId(terminalJob.jobId);
      });

    return () => {
      subscribed = false;
    };
  }, [terminalJob?.jobId, terminalJob?.updatedAt]);

  if (!terminalJob) return null;

  const details = loaded?.jobId === terminalJob.jobId ? loaded.details : null;
  if (!details) {
    const failed = failedJobId === terminalJob.jobId;
    return (
      <section
        data-testid="data-transfer-result"
        data-job-id={terminalJob.jobId}
        data-restored="true"
        data-verdict-severity="uncertain"
        data-completed="false"
        data-replayed="false"
        data-cancel-disposition="none"
        data-requires-reconcile="true"
        className="rounded border border-warning/30 bg-warning/5 px-4 py-3 text-sm text-fg"
      >
        <p role="status">
          {failed ? t('transfer.job.detailsUnavailable') : t('transfer.job.detailsLoading')}
        </p>
      </section>
    );
  }

  const { job } = details;
  const recovery = details.recovery;
  const effectOutcome = job.effectOutcome ?? 'unknown';
  const verdict = deriveMigrationJobVerdict({
    state: job.state,
    effectOutcome,
    cancelRequested: job.cancelRequested,
    commitBoundaries: details.commitBoundaries,
    recoveryVerdict: recovery?.verdict ?? null,
    recoveryReason: recovery?.reasonCode ?? null,
    error: job.error ?? null,
    committedRows: readJobCounter(job.progress.committed),
    unknownRows: readJobCounter(job.progress.unknown),
  });
  const plan = {
    planId: details.planId ?? null,
    planDigest: details.planDigest ?? null,
    selectionRevision: details.selectionRevision ?? null,
  };

  return (
    <section
      data-testid="data-transfer-result"
      data-job-id={job.jobId}
      data-restored="true"
      data-verdict-severity={verdict.severity}
      data-completed={String(verdict.completed)}
      data-replayed="false"
      data-cancel-disposition={verdict.cancelDisposition}
      data-requires-reconcile={String(verdict.requiresReconcile)}
      data-plan-id={plan.planId ?? undefined}
      className="flex flex-col gap-2"
    >
      <p className="text-xs font-medium text-fg-muted">{t('transfer.job.restoredTitle')}</p>
      <MigrationJobVerdictPanel
        verdict={verdict}
        progress={job.progress}
        commitBoundaries={details.commitBoundaries}
        artifactIds={job.artifactIds}
        cancelRequested={job.cancelRequested}
        cancelAcknowledged={job.cancelRequested}
        recoveryVerdict={recovery?.verdict ?? null}
        recoveryReason={recovery?.reasonCode ?? null}
        recoveryResumeThrough={recovery?.resumeThrough ?? null}
        error={job.error ?? null}
        plan={plan}
        testIdPrefix="data-transfer-job"
      />
    </section>
  );
}
