import { useCallback, useEffect, useMemo, useState } from 'react';
import { syncCommands, type DataSyncJobDetails } from '../../commands/sync';
import { useI18n } from '../../hooks/useI18n';
import type { DataSyncEndpoints } from './useDataSyncEndpoints';
import type { DataSyncFeedback } from './useDataSyncFeedback';

export interface UseDataSyncJobLifecycleOptions {
  endpoints: DataSyncEndpoints;
  feedback: DataSyncFeedback;
}

/**
 * Durable job list of the Data Sync window.
 *
 * The backend keeps a durable receipt of every comparison / execution, so the
 * window polls the job list and can replay the recovery of a job whose write
 * outcome was never confirmed.
 */
export function useDataSyncJobLifecycle({
  endpoints,
  feedback,
}: UseDataSyncJobLifecycleOptions) {
  const { t } = useI18n();
  const { setErrorMsg, setErrorOpen, setStatusMsg } = feedback;
  const {
    sourceId,
    targetId,
    sourceDatabase,
    targetDatabase,
    sourceSchema,
    targetSchema,
    refreshEndpointSessions,
  } = endpoints;

  const [durableJobs, setDurableJobs] = useState<DataSyncJobDetails[]>([]);
  const [verifyingRecoveryJob, setVerifyingRecoveryJob] = useState<string | null>(null);
  const [cancellingDurableJob, setCancellingDurableJob] = useState<string | null>(null);

  const refreshDataSyncJobs = useCallback(async () => {
    try {
      const jobs = await syncCommands.listDataSyncJobs();
      setDurableJobs(jobs);
      return jobs;
    } catch {
      return null;
    }
  }, []);

  useEffect(() => {
    void refreshDataSyncJobs();
    const timer = window.setInterval(() => void refreshDataSyncJobs(), 2000);
    return () => window.clearInterval(timer);
  }, [refreshDataSyncJobs]);

  const notExecutedJobs = useMemo(
    () => durableJobs.filter((job) => job.recovery?.verdict === 'notExecuted'),
    [durableJobs],
  );

  const verifyDataSyncJobRecovery = useCallback(
    async (candidate: DataSyncJobDetails) => {
      if (candidate.recovery?.verdict !== 'pendingVerification') return;
      const [sourceTarget, targetTarget] = candidate.recoveryTargets;
      if (
        !sourceTarget ||
        !targetTarget ||
        sourceTarget.connectionId !== sourceId ||
        targetTarget.connectionId !== targetId
      ) {
        setErrorMsg(t('migrationJob.recoveryEndpointMismatch'));
        setErrorOpen(true);
        return;
      }
      if (!sourceDatabase || !targetDatabase) {
        setErrorMsg(t('migrationJob.recoverySelectEndpoints'));
        setErrorOpen(true);
        return;
      }

      setVerifyingRecoveryJob(candidate.job.jobId);
      try {
        const { source, target } = await refreshEndpointSessions();
        if (!source?.dbSessionId || !target?.dbSessionId) {
          throw new Error(t('migrationJob.recoverySelectEndpoints'));
        }
        const verified = await syncCommands.verifyDataSyncRecovery({
          jobId: candidate.job.jobId,
          sourceDbSessionId: source.dbSessionId,
          targetDbSessionId: target.dbSessionId,
          sourceDatabase,
          targetDatabase,
          sourceSchema: sourceSchema || undefined,
          targetSchema: targetSchema || undefined,
        });
        refreshDataSyncJobs();
        setStatusMsg(
          verified.recovery?.verdict === 'requireManualReview'
            ? t('migrationJob.recoveryManualReview')
            : t('migrationJob.recoveryRecorded'),
        );
      } catch (error) {
        setErrorMsg(error instanceof Error ? error.message : String(error));
        setErrorOpen(true);
      } finally {
        setVerifyingRecoveryJob(null);
      }
    },
    [
      refreshEndpointSessions,
      refreshDataSyncJobs,
      sourceDatabase,
      sourceId,
      sourceSchema,
      targetDatabase,
      targetId,
      targetSchema,
      t,
    ],
  );

  const cancelDurableDataSyncJob = useCallback(
    async (jobId: string) => {
      setCancellingDurableJob(jobId);
      try {
        await syncCommands.cancelDataSync(jobId);
        await refreshDataSyncJobs();
      } catch (error) {
        setErrorMsg(error instanceof Error ? error.message : String(error));
        setErrorOpen(true);
      } finally {
        setCancellingDurableJob(null);
      }
    },
    [refreshDataSyncJobs],
  );

  return {
    durableJobs,
    notExecutedJobs,
    verifyingRecoveryJob,
    cancellingDurableJob,
    refreshDataSyncJobs,
    verifyDataSyncJobRecovery,
    cancelDurableDataSyncJob,
  };
}

export type DataSyncJobLifecycle = ReturnType<typeof useDataSyncJobLifecycle>;
