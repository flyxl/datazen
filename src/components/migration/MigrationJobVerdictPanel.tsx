/**
 * Shared verdict surface for the P5 migration tools.
 *
 * Data Transfer, Schema Diff and Data Sync all render "how did this run
 * actually end" from the same facts — an `EffectOutcome`, a list of
 * `CommitBoundary` records, and a recovery verdict — so this component is
 * deliberately domain-free. It takes a {@link MigrationJobVerdict} plus the
 * raw counters it needs to display and never invents a conclusion the
 * backend did not reach.
 *
 * The rendering rules it exists to enforce:
 *
 * - "cancel requested" is shown as an intent, next to where the run stopped.
 * - A boundary with no evidence is shown as unverified, never as committed.
 * - An uncertain verdict is phrased as uncertain; there is no `severity`
 *   branch that turns uncertainty into success.
 *
 * @param testIdPrefix binds `data-*` attributes for WDIO specs.
 */

import { Badge } from '../ui/Badge';
import { CopyableError } from '../ui/CopyableError';
import { useI18n } from '../../hooks/useI18n';
import {
  readJobCounter,
  sumJobCounters,
  type MigrationJobVerdict,
  type VerdictSeverity,
} from '../../lib/migrationJobVerdict';
import type { CommitBoundary, JobProgress } from '@datazen/backend-client';

const SEVERITY_CLASS: Record<VerdictSeverity, string> = {
  ok: 'text-success',
  partial: 'text-warning',
  uncertain: 'text-warning',
  failed: 'text-danger',
};

export interface MigrationJobVerdictPanelProps {
  verdict: MigrationJobVerdict;
  /** Raw five-bucket progress. Counters arrive as decimal strings. */
  progress?: JobProgress | null;
  commitBoundaries?: readonly CommitBoundary[];
  /** SQL-file runs: `transfer-sql-<sha256hex>`. */
  artifactIds?: readonly string[];
  cancelRequested?: boolean;
  /** `false` means the backend had no such Job — a cancel that did nothing. */
  cancelAcknowledged?: boolean;
  /** This Job is a replayed receipt, not a fresh write. */
  replayed?: boolean;
  recoveryVerdict?: string | null;
  recoveryReason?: string | null;
  /**
   * How many stages recovery considers already committed. `null` means
   * the backend decided nothing is safe to resume; it is not the same as `0`.
   */
  recoveryResumeThrough?: number | null;
  error?: string | null;
  /**
   * Plan identity of the settled Job. Surfaced as `data-*` (not as copy) so a
   * spec — and a support ticket — can name the exact plan this verdict belongs
   * to; all three tools mint one planId per review.
   */
  plan?: {
    planId?: string | null;
    planDigest?: string | null;
    /** u64 revision of the frozen selection, as the backend reports it. */
    selectionRevision?: number | null;
  } | null;
  testIdPrefix?: string;
  className?: string;
}

function severityKey(severity: VerdictSeverity): string {
  switch (severity) {
    case 'ok':
      return 'migration.verdict.ok';
    case 'partial':
      return 'migration.verdict.partial';
    case 'uncertain':
      return 'migration.verdict.uncertain';
    case 'failed':
      return 'migration.verdict.failed';
  }
}

function cancelDispositionKey(verdict: MigrationJobVerdict): string {
  switch (verdict.cancelDisposition) {
    case 'none':
      return 'migration.cancel.none';
    case 'notStarted':
      return 'migration.cancel.notStarted';
    case 'requestedInFlight':
      return 'migration.cancel.requestedInFlight';
    case 'settledPartially':
      return 'migration.cancel.settledPartially';
    case 'settledRolledBack':
      return 'migration.cancel.settledRolledBack';
    case 'settledCompleted':
      return 'migration.cancel.settledCompleted';
    case 'settledUnknown':
      return 'migration.cancel.settledUnknown';
  }
}

function uncertaintyKey(verdict: MigrationJobVerdict): string | null {
  switch (verdict.uncertainty) {
    case 'none':
      return null;
    case 'noCheckpoint':
      return 'migration.uncertainty.noCheckpoint';
    case 'recoveryRejected':
      return 'migration.uncertainty.recoveryRejected';
    case 'effectOutcomeUnknown':
      return 'migration.uncertainty.effectOutcomeUnknown';
    case 'missingEvidence':
      return 'migration.uncertainty.missingEvidence';
    case 'manualReviewRequired':
      return 'migration.uncertainty.manualReviewRequired';
  }
}

export function MigrationJobVerdictPanel({
  verdict,
  progress,
  commitBoundaries,
  artifactIds,
  cancelRequested,
  cancelAcknowledged,
  replayed,
  recoveryVerdict,
  recoveryReason,
  recoveryResumeThrough,
  error,
  plan = null,
  testIdPrefix = 'migration-job',
  className,
}: MigrationJobVerdictPanelProps) {
  const { t } = useI18n();
  const boundaries = commitBoundaries ?? [];
  const artifacts = artifactIds ?? [];
  const committedRows = sumJobCounters(progress?.committed, progress?.unknown);
  // Counts, not lists: the verdict folds the boundaries into two numbers so the
  // shared engine stays free of the wire-level `CommitBoundary` shape.
  const verified = verdict.verifiedBoundaries;
  const unverified = verdict.unverifiedBoundaries;
  const uncertaintyLabel = uncertaintyKey(verdict);
  const recoveryReasonIsRestartBeforeDispatch =
    recoveryVerdict === 'notExecuted' && recoveryReason === 'notDispatchedAfterRestart';

  return (
    <div
      data-testid={`${testIdPrefix}-verdict`}
      data-severity={verdict.severity}
      data-cancel-disposition={verdict.cancelDisposition}
      data-uncertainty={verdict.uncertainty}
      data-verified-boundaries={verified}
      data-unverified-boundaries={unverified}
      data-plan-id={plan?.planId ?? undefined}
      data-plan-digest={plan?.planDigest ?? undefined}
      data-selection-revision={plan?.selectionRevision ?? undefined}
      className={className ?? 'space-y-3 rounded-lg border border-edge bg-surface-alt p-6 text-sm'}
    >
      <p
        role="status"
        data-testid={`${testIdPrefix}-verdict-status`}
        className={`text-base font-medium ${SEVERITY_CLASS[verdict.severity]}`}
      >
        {t(severityKey(verdict.severity))}
      </p>

      {/* Intent and outcome are two different sentences on purpose. */}
      <p
        data-testid={`${testIdPrefix}-cancel`}
        data-cancel-requested={cancelRequested === true}
        data-cancel-acknowledged={cancelAcknowledged === true}
        className="text-fg-muted"
      >
        {t(cancelDispositionKey(verdict))}
      </p>
      {cancelRequested && cancelAcknowledged === false ? (
        <p
          data-testid={`${testIdPrefix}-cancel-unknown-job`}
          className="text-warning"
        >
          {t('migration.cancel.unknownJob')}
        </p>
      ) : null}

      {progress ? (
        <p data-testid={`${testIdPrefix}-progress`}>
          {t('migration.progress.counts')}: {readJobCounter(progress.read) ?? 0}
          {' / '}
          {readJobCounter(progress.converted) ?? 0}
          {' / '}
          {readJobCounter(progress.attempted) ?? 0}
          {' / '}
          {readJobCounter(progress.committed) ?? 0}
          {' / '}
          {readJobCounter(progress.unknown) ?? 0}
        </p>
      ) : null}

      {/* Unknown rows are counted beside committed ones, never folded into
          them — the difference is what tells a user to reconcile. */}
      <p data-testid={`${testIdPrefix}-rows`}>
        {t('migration.verdict.committedRows')}: {committedRows}
      </p>

      {replayed ? (
        <p data-testid={`${testIdPrefix}-replayed`} className="text-fg-muted">
          {t('migration.verdict.replayedReceipt')}
        </p>
      ) : null}

      {boundaries.length > 0 ? (
        <ul data-testid={`${testIdPrefix}-boundaries`} className="space-y-1">
          {boundaries.map((boundary) => {
            const isVerified = boundary.evidence.length > 0;
            return (
              <li
                key={`${boundary.stageId}:${boundary.committedAt}`}
                data-testid={`${testIdPrefix}-boundary-${boundary.stageId}`}
                data-boundary-verified={isVerified}
                className="rounded border border-edge bg-surface p-2 text-xs"
              >
                <span className="font-medium">{boundary.stageId}</span>
                {' · '}
                <span className={isVerified ? 'text-success' : 'text-warning'}>
                  {isVerified
                    ? t('migration.boundary.verified')
                    : t('migration.boundary.unverified')}
                </span>
                {boundary.operationId ? ` · ${boundary.operationId}` : ''}
              </li>
            );
          })}
        </ul>
      ) : (
        // An empty boundary list only means "nothing was written" when nothing
        // was. `data-evidence-gap` marks the other case, where rows were
        // committed but no boundary marker backs them — the state the SQL-file
        // path produces, because it mints an artifact instead of row commits
        // and therefore records no boundaries.
        <p
          data-testid={`${testIdPrefix}-boundaries-empty`}
          data-evidence-gap={verdict.uncertainty === 'missingEvidence'}
          className={verdict.uncertainty === 'missingEvidence' ? 'text-warning' : 'text-fg-muted'}
        >
          {t('migration.verdict.noBoundaries')}
        </p>
      )}

      {/* A SQL-file run has no target rows to count, so the artifact id it
          minted is the only proof the script was produced. It is shown rather
          than swallowed, or the run would look like it did nothing. */}
      {artifacts.length > 0 ? (
        <ul data-testid={`${testIdPrefix}-artifacts`} className="space-y-1">
          {artifacts.map((artifactId) => (
            <li
              key={artifactId}
              data-testid={`${testIdPrefix}-artifact`}
              data-artifact-id={artifactId}
              className="rounded border border-edge bg-surface p-2 font-mono text-xs"
            >
              {artifactId}
            </li>
          ))}
        </ul>
      ) : null}

      {recoveryVerdict ? (
        <p data-testid={`${testIdPrefix}-recovery-verdict`} data-verdict={recoveryVerdict}>
          {t('migration.verdict.recoveryLabel')}:{' '}
          {recoveryVerdict === 'notExecuted'
            ? t('migration.verdict.notExecuted')
            : recoveryVerdict}
        </p>
      ) : null}
      {recoveryReason ? (
        <p
          data-testid={`${testIdPrefix}-recovery-reason`}
          data-reason-code={recoveryReason}
          className="text-fg-muted"
        >
          {recoveryReasonIsRestartBeforeDispatch
            ? t('migration.verdict.notDispatchedAfterRestart')
            : recoveryReason}
        </p>
      ) : null}
      {recoveryVerdict && recoveryResumeThrough !== null && recoveryResumeThrough !== undefined ? (
        <p data-testid={`${testIdPrefix}-recovery-resume-through`} className="text-fg-muted">
          {t('migration.verdict.resumeThrough')}: {recoveryResumeThrough}
        </p>
      ) : null}

      {/* Uncertainty is stated in words. There is deliberately no path that
          renders it as a completed run. */}
      {uncertaintyLabel ? (
        <Badge data-testid={`${testIdPrefix}-uncertainty`} tone="warning">
          {t(uncertaintyLabel)}
        </Badge>
      ) : null}

      {verdict.requiresReconcile ? (
        <p data-testid={`${testIdPrefix}-reconcile`} className="text-warning">
          {t('migration.verdict.requiresReconcile')}
        </p>
      ) : null}

      {error ? (
        <CopyableError
          message={error}
          className="error-message text-xs"
          copyButton
          data-testid={`${testIdPrefix}-error`}
        />
      ) : null}
    </div>
  );
}

export default MigrationJobVerdictPanel;
