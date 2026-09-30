/**
 * §4.3 Explicit completion-cache refresh.
 *
 * Picks up tables created outside DataZen without a reconnect or restart, which
 * previously required switching databases to force a re-read.
 */
import { useCallback, useState } from 'react';
import { schemaClient } from '@datazen/driver-sdk';
import { RefreshCw } from 'lucide-react';
import { ToolbarButton } from '../../../../components/ui/ToolbarButton';
import { invalidateSchemaCache } from '../../../../lib/schemaCache';
import { useSchemaStore } from '../../../../stores/schemaStore';
import { useI18n } from '../../../../hooks/useI18n';
import { tid } from '../../../../lib/tid';

export interface RefreshCompletionButtonProps {
  dbSessionId: string;
  database?: string | null;
  /** Icon-only trigger; the full name stays in the tooltip / aria-label. */
  iconOnly?: boolean;
  disabled?: boolean;
  onRefreshed: (message: string) => void;
}

export function RefreshCompletionButton({
  dbSessionId,
  database,
  iconOnly,
  disabled,
  onRefreshed,
}: RefreshCompletionButtonProps) {
  const { t } = useI18n();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleRefresh = useCallback(async () => {
    if (!dbSessionId || busy) return;
    setBusy(true);
    setError(null);
    try {
      // Drop the underlying table-schema/DDL layer first: clearing only the
      // editor snapshot would let it repopulate from the same stale entries.
      invalidateSchemaCache(dbSessionId);
      await schemaClient.refresh(dbSessionId, { kind: 'session' });
      // Reads started while the backend was refreshing are stale as well.
      invalidateSchemaCache(dbSessionId);
      if (database) {
        await useSchemaStore.getState().loadTables(database, dbSessionId);
      }
      onRefreshed(t('query.refreshCompletionDone'));
    } catch (error) {
      setError(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false);
    }
  }, [dbSessionId, database, busy, onRefreshed, t]);

  return (
    <ToolbarButton
      iconOnly={iconOnly}
      variant="ghost"
      label={t('query.refreshCompletion')}
      title={error ?? t('query.refreshCompletionTitle')}
      icon={<RefreshCw className={`h-3.5 w-3.5${busy ? ' animate-spin' : ''}`} />}
      onClick={() => void handleRefresh()}
      disabled={disabled || busy || !dbSessionId}
      {...tid('editor-refresh-completion-button')}
    />
  );
}
