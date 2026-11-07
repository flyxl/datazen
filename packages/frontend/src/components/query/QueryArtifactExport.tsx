import { useRef, useState } from 'react';
import { Download } from 'lucide-react';
import { artifactCommands } from '../../commands/artifact';
import { useI18n } from '../../hooks/useI18n';
import type { QueryResultArtifacts } from '@datazen/backend-client';

/** Export the original published execution, including cancelled/failed prefixes. */
export function QueryArtifactExport({ artifacts }: { artifacts: QueryResultArtifacts }) {
  const { t } = useI18n();
  const inFlight = useRef(false);
  const [busy, setBusy] = useState(false);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const artifactIds = [...new Set(artifacts.artifactIds)];
  if (artifacts.resultCompleteness === 'pending' || artifactIds.length === 0) return null;

  const exportResult = async (artifactId: string) => {
    if (inFlight.current) return;
    inFlight.current = true;
    setBusy(true);
    setSaved(false);
    setError(null);
    try {
      const receipt = await artifactCommands.exportArtifact(artifactId);
      // Cancelled native confirmation/save dialogs are a normal exit.
      if (receipt.status === 'saved') setSaved(true);
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : t('query.artifactExport.failed'));
    } finally {
      inFlight.current = false;
      setBusy(false);
    }
  };

  return (
    <div
      className="flex shrink-0 flex-wrap items-center gap-2 border-b border-edge bg-surface-alt px-3 py-1.5 text-xs"
      data-testid="query-artifact-export"
    >
      {artifactIds.map((artifactId, index) => (
        <button
          key={artifactId}
          type="button"
          disabled={busy}
          className="inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-fg-secondary hover:bg-surface-raised hover:text-fg disabled:opacity-50"
          data-testid="query-artifact-export-button"
          data-artifact-index={index}
          onClick={() => void exportResult(artifactId)}
        >
          <Download className="h-3 w-3" />
          {artifactIds.length === 1
            ? t('query.artifactExport.action')
            : t('query.artifactExport.numberedAction', { index: index + 1 })}
        </button>
      ))}
      {artifacts.resultCompleteness === 'truncated' && <span className="text-warning">{t('query.artifactExport.partial')}</span>}
      {busy && <span role="status">{t('query.artifactExport.saving')}</span>}
      {saved && <span role="status" data-testid="query-artifact-export-saved">{t('query.artifactExport.saved')}</span>}
      {error && <span role="alert" data-testid="query-artifact-export-error" className="text-danger">{error}</span>}
    </div>
  );
}
