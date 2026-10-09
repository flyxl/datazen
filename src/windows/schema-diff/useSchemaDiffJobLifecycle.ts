import { useCallback, useEffect, useState } from 'react';

import type { JobView } from '@datazen/backend-client';
import {
  schemaDiffCommands,
  subscribeSchemaDiffJobUpdates,
  type SchemaDiffJobAccepted,
  type SchemaDiffJobDetails,
} from '../../commands/schemaDiff';

function isActive(job: JobView): boolean {
  return job.state === 'queued' || job.state === 'running';
}

function latestFirst(jobs: readonly JobView[]): JobView[] {
  return [...jobs].sort((left, right) => right.updatedAt - left.updatedAt);
}

export interface SchemaDiffTrackedJob extends SchemaDiffJobAccepted {
  details: SchemaDiffJobDetails | null;
  /** Only jobs submitted in this wizard may populate its execution result. */
  submittedLocally?: boolean;
}

export function useSchemaDiffJobLifecycle() {
  const [currentJob, setCurrentJob] = useState<SchemaDiffTrackedJob | null>(null);
  const [latestApply, setLatestApply] = useState<SchemaDiffJobDetails | null>(null);
  const [cancelOutcome, setCancelOutcome] = useState<'idle' | 'requested' | 'unavailable'>('idle');
  const [verifying, setVerifying] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const accept = useCallback((accepted: SchemaDiffJobAccepted) => {
    setCancelOutcome('idle');
    setCurrentJob({ ...accepted, details: null, submittedLocally: true });
  }, []);

  const observe = useCallback((details: SchemaDiffJobDetails) => {
    const view = details.details.job;
    const accepted: SchemaDiffJobAccepted = {
      jobId: view.jobId,
      kind: view.kind as SchemaDiffJobAccepted['kind'],
      state: view.state,
      progress: view.progress,
    };
    setCurrentJob((current) =>
      current && current.jobId !== view.jobId
        ? current
        : { ...accepted, details, submittedLocally: current?.submittedLocally ?? false },
    );
    if (view.kind === 'schemaDiffApply') setLatestApply(details);
  }, []);

  useEffect(
    () =>
      subscribeSchemaDiffJobUpdates((update) => {
        if (update.type === 'accepted') accept(update.accepted);
        else observe(update.details);
      }),
    [accept, observe],
  );

  useEffect(() => {
    let stopped = false;
    let timer: number | undefined;
    const read = async (jobId: string): Promise<SchemaDiffJobDetails | null> => {
      try {
        const details = await schemaDiffCommands.getJobDetails(jobId);
        if (stopped) return null;
        observe(details);
        return details;
      } catch (cause) {
        if (!stopped) setError(cause instanceof Error ? cause.message : String(cause));
        return null;
      }
    };
    const poll = async (jobId: string) => {
      const details = await read(jobId);
      if (stopped || !details || !isActive(details.details.job)) return;
      timer = window.setTimeout(() => void poll(jobId), 1000);
    };

    void (async () => {
      try {
        const jobs = latestFirst(await schemaDiffCommands.listJobs());
        if (stopped || jobs.length === 0) return;
        const active = jobs.find(isActive);
        const pending = jobs.find((job) => job.pendingVerificationReason !== null);
        const apply = jobs.find((job) => job.kind === 'schemaDiffApply');
        const prepare = jobs.find((job) => job.kind === 'schemaDiffPrepare');
        const selected = active ?? pending ?? apply ?? prepare;
        const detailsToRead = new Set<string>();
        if (selected) detailsToRead.add(selected.jobId);
        if (apply) detailsToRead.add(apply.jobId);
        for (const jobId of detailsToRead) await read(jobId);
        if (active) await poll(active.jobId);
      } catch (cause) {
        if (!stopped) setError(cause instanceof Error ? cause.message : String(cause));
      }
    })();

    return () => {
      stopped = true;
      if (timer !== undefined) window.clearTimeout(timer);
    };
  }, [observe]);

  const requestCancel = useCallback(async () => {
    const job = currentJob;
    if (!job || (job.state !== 'queued' && job.state !== 'running')) return false;
    setCancelOutcome('requested');
    try {
      const acknowledged = await schemaDiffCommands.cancelDeploy(job.jobId);
      if (!acknowledged) setCancelOutcome('unavailable');
      return acknowledged;
    } catch (cause) {
      setCancelOutcome('unavailable');
      setError(cause instanceof Error ? cause.message : String(cause));
      return false;
    }
  }, [currentJob]);

  const verifyRecovery = useCallback(
    async (jobId: string, targetDbSessionId: string) => {
      setVerifying(true);
      setError(null);
      try {
        const details = await schemaDiffCommands.verifyRecovery(jobId, targetDbSessionId);
        observe(details);
        return details;
      } catch (cause) {
        setError(cause instanceof Error ? cause.message : String(cause));
        return null;
      } finally {
        setVerifying(false);
      }
    },
    [observe],
  );

  const showAccepted = useCallback(accept, [accept]);
  const showDetails = useCallback(observe, [observe]);

  return {
    currentJob,
    latestApply,
    cancelOutcome,
    verifying,
    error,
    requestCancel,
    verifyRecovery,
    showAccepted,
    showDetails,
  };
}
