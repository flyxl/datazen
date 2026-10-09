/**
 * Per-window job hydration for the three migration windows.
 *
 * Mounts with a job-center-first read: listJobs → getJob. Re-subscribing
 * to an active job happens through `watchJob` if the window opts in.
 *
 * Unmount rule: the cleanup only flags the loop as cancelled and stops
 * the watch. No `cancelJob`, `removeJob` or session `release` call may be
 * made here — a window closing is an unsubscribe, never a resource release.
 */

import { useEffect, useRef, useState } from 'react';

import {
  boundBackendClient,
  hydrateMigrationJobs,
  MIGRATION_JOB_KINDS,
  updateMigrationJobProjection,
  type MigrationJobHydration,
  type MigrationWindow,
} from '../lib/migrationJobHydration';

export interface UseMigrationJobHydrationResult {
  hydration: MigrationJobHydration | null;
  hydrationError: string | null;
}

export function useMigrationJobHydration(
  window: MigrationWindow,
): UseMigrationJobHydrationResult {
  const [hydration, setHydration] = useState<MigrationJobHydration | null>(null);
  const [hydrationError, setHydrationError] = useState<string | null>(null);
  // Ref so the unmount cleanup can flip it without re-running the effect.
  const cancelledRef = useRef(false);

  useEffect(() => {
    cancelledRef.current = false;
    let unsubscribed = false;
    const watchHandles: Array<{ stop(): void }> = [];

    const client = boundBackendClient();
    if (!client) {
      setHydrationError(null);
      setHydration({ jobs: [], activeJobs: [], terminalJobs: [], verificationJobs: [] });
      return () => {
        unsubscribed = true;
      };
    }

    void (async () => {
      try {
        const result = await hydrateMigrationJobs(client, window);
        if (unsubscribed || cancelledRef.current) return;
        setHydration(result);
        setHydrationError(null);
        // Re-subscribe to each active job's view; on error the watch loop
        // re-fetches on its next tick (re-subscribe semantics). Each update
        // replaces the job in the returned projection, including moving it
        // from active to terminal when the backend settles it.
        for (const job of result.activeJobs) {
          if (unsubscribed) break;
          const handleState = { stopped: false };
          const watch = client.watchJob(
            job.jobId,
            (latest) => {
              if (unsubscribed || cancelledRef.current) return;
              setHydration((current) =>
                current ? updateMigrationJobProjection(current, latest) : current,
              );
              if (latest.state !== 'queued' && latest.state !== 'running') {
                if (!handleState.stopped) {
                  handleState.stopped = true;
                  watch?.stop();
                }
              }
            },
            {
              intervalMs: 2000,
              onError: () => undefined,
            },
          );
          watchHandles.push({
            stop: () => {
              if (handleState.stopped) return;
              handleState.stopped = true;
              watch.stop();
            },
          });
        }
      } catch (error) {
        if (unsubscribed || cancelledRef.current) return;
        setHydrationError(
          error instanceof Error ? error.message : String(error),
        );
      }
    })();

    return () => {
      unsubscribed = true;
      cancelledRef.current = true;
      // Unsubscribe only: stop local polling. Never cancel the job itself.
      for (const handle of watchHandles) handle.stop();
    };
  }, [window]);

  return { hydration, hydrationError };
}

export { MIGRATION_JOB_KINDS };
