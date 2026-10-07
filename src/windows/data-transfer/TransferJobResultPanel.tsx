import { MigrationJobFailureNotice } from '../../components/migration/MigrationJobFailureNotice';
import { MigrationJobVerdictPanel } from '../../components/migration/MigrationJobVerdictPanel';
import { useLocaleDomains } from '../../hooks/useLocaleDomains';
import type { TransferJobRun } from '../../hooks/useTransferJobRun';

/**
 * §2.3 / §6.1 / §7 / §10 result surface for Data Transfer.
 *
 * The rendering is shared; only the plumbing is transfer-specific. Everything
 * displayed comes from {@link MigrationJobVerdictPanel} and the verdict engine,
 * so Schema Diff and Data Sync reuse this component by supplying their own run
 * object with the same field names — no verdict logic is re-implemented here.
 */
export interface TransferJobResultPanelProps {
  run: TransferJobRun;
  /** §9: the plan was spent, so the only legal way forward is a fresh review. */
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
  const { applyView, failure, verdict } = run;
  // §9: the plan identity travels with the verdict so the panel can name the
  // exact plan that was spent. Same three fields for all three tools.
  const plan = {
    planId: applyView?.planId ?? null,
    planDigest: applyView?.planDigest ?? null,
    selectionRevision: applyView?.selectionRevision ?? null,
  };

  return (
    <section
      data-testid="data-transfer-result"
      data-verdict-severity={verdict?.severity ?? 'failed'}
      data-completed={String(verdict?.completed ?? false)}
      data-replayed={String(applyView?.replayed ?? false)}
      data-cancel-disposition={verdict?.cancelDisposition ?? 'none'}
      data-requires-reconcile={String(verdict?.requiresReconcile ?? false)}
      data-plan-id={plan.planId ?? undefined}
      className="flex flex-col gap-4"
    >
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
        §8 / §6.2 refusals land here. Drawing the panel anyway would imply a
        run happened, so the refusal is shown on its own.
      */}
      {verdict ? (
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