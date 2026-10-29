import { CopyableError } from '../../components/ui/CopyableError';
import type { SchemaDiffJobDetails } from '../../commands/schemaDiff';
import { SchemaDiffJobStatusPanel } from './SchemaDiffJobStatusPanel';
import type { SchemaDiffTrackedJob } from './useSchemaDiffJobLifecycle';

export function SchemaDiffWindowJobSection({
  currentJob,
  latestApply,
  cancelOutcome,
  verifying,
  error,
  targetReady,
  onCancel,
  onVerify,
}: {
  currentJob: SchemaDiffTrackedJob | null;
  latestApply: SchemaDiffJobDetails | null;
  cancelOutcome: 'idle' | 'requested' | 'unavailable';
  verifying: boolean;
  error: string | null;
  targetReady: boolean;
  onCancel: () => void;
  onVerify: (jobId: string) => void;
}) {
  return (
    <>
      <SchemaDiffJobStatusPanel
        currentJob={currentJob}
        latestApply={latestApply}
        cancelOutcome={cancelOutcome}
        verifying={verifying}
        targetReady={targetReady}
        onCancel={onCancel}
        onVerify={onVerify}
      />
      {error && (
        <CopyableError
          message={error}
          className="error-message border-b border-edge px-6 py-2 text-xs"
        />
      )}
    </>
  );
}
