import { MigrationJobFailureNotice } from '../../components/migration/MigrationJobFailureNotice';
import { MigrationJobVerdictPanel } from '../../components/migration/MigrationJobVerdictPanel';
import { useLocaleDomains } from '../../hooks/useLocaleDomains';
import { useI18n } from '../../hooks/useI18n';
import { readJobCounter } from '../../lib/migrationJobVerdict';
import type { TransferJobRun } from '../../hooks/useTransferJobRun';

/**
 * Result surface for Data Transfer.
 *
 * The rendering is shared; only the plumbing is transfer-specific. Everything
 * displayed comes from {@link MigrationJobVerdictPanel} and the verdict engine,
 * so Schema Diff and Data Sync reuse this component by supplying their own run
 * object with the same field names — no verdict logic is re-implemented here.
 */
export interface TransferJobResultPanelProps {
  run: TransferJobRun;
  /** The plan was spent, so the only legal way forward is a fresh review. */
  onReReview: () => void;
  testIdPrefix?: string;
}

export function TransferJobResultPanel({
  run,
  onReReview,
  testIdPrefix = 'migration-job',
}: TransferJobResultPanelProps) {
  // The verdict keys live in the lazily-loaded `sync` domain pack.
  useLocaleDomains(['sync']);
  const { t } = useI18n();
  const { applyView, failure, verdict } = run;
  // The plan identity travels with the verdict so the panel can name the
  // exact plan that was spent. Same three fields for all three tools.
  const plan = {
    planId: applyView?.planId ?? null,
    planDigest: applyView?.planDigest ?? null,
    selectionRevision: applyView?.selectionRevision ?? null,
  };
  const activeProgress = applyView
    ? [
        applyView.progress.read,
        applyView.progress.converted,
        applyView.progress.attempted,
        applyView.progress.committed,
        applyView.progress.unknown,
      ]
        .map((value) => readJobCounter(value) ?? 0)
        .join(' / ')
    : null;

  return (
    <section
      data-testid="data-transfer-result"
      data-verdict-severity={run.isInFlight ? 'active' : verdict?.severity ?? 'failed'}
      data-completed={String(verdict?.completed ?? false)}
      data-replayed={String(applyView?.replayed ?? false)}
      data-cancel-disposition={verdict?.cancelDisposition ?? 'none'}
      data-requires-reconcile={String(verdict?.requiresReconcile ?? false)}
      data-plan-id={plan.planId ?? undefined}
      className="flex flex-col gap-4"
    >
      {run.isInFlight && applyView ? (
        <div className="rounded border border-accent/30 bg-accent/5 px-3 py-2 text-sm text-fg">
          <p
            role="status"
            data-testid="data-transfer-job-active-state"
            data-job-id={applyView.jobId}
            data-state={applyView.state}
          >
            {t(`transfer.job.${applyView.state}`)}
          </p>
          <p data-testid="data-transfer-job-active-progress" className="text-xs text-fg-muted">
            {t('migration.progress.counts')}: {activeProgress}
          </p>
          {run.cancelRequested ? (
            <p data-testid="data-transfer-job-active-cancel" className="text-xs text-warning">
              {t('migration.cancel.requestedInFlight')}
            </p>
          ) : null}
        </div>
      ) : null}
      {/* The refusal notice shares the prefix, so a spec can query one
          namespace for both the refusal and the settled verdict. */}
      {failure ? (
        <MigrationJobFailureNotice
          failure={failure}
          onReReview={onReReview}
          testIdPrefix={testIdPrefix}
        />
      ) : null}

      {/*
        No verdict means the backend never admitted a Job to report on — the
        backend-scope and pipeline-budget refusals land here. Drawing the panel
        anyway would imply a run happened, so the refusal is shown on its own.
      */}
      {verdict && !run.isInFlight ? (
        <MigrationJobVerdictPanel
          verdict={verdict}
          progress={applyView?.progress ?? null}
          commitBoundaries={applyView?.commitBoundaries ?? undefined}
          artifactIds={applyView?.artifactIds ?? undefined}
          cancelRequested={run.cancelRequested}
          cancelAcknowledged={run.cancelAcknowledged}
          replayed={applyView?.replayed ?? false}
          recoveryVerdict={applyView?.recoveryVerdict ?? null}
          recoveryReason={applyView?.recoveryReason ?? null}
          recoveryResumeThrough={applyView?.recoveryResumeThrough ?? null}
          error={applyView?.error ?? null}
          plan={plan}
          testIdPrefix={testIdPrefix}
        />
      ) : null}
    </section>
  );
}

export default TransferJobResultPanel;
