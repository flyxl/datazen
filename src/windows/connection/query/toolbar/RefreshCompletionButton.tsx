/**
 * §4.3 Explicit completion-cache refresh.
 *
 * Picks up tables created outside DataZen without a reconnect or restart, which
 * previously required switching databases to force a re-read.
 */
import { useCallback, useState } from 'react';
import { RefreshCw } from 'lucide-react';
import { ToolbarButton } from '../../../../components/ui/ToolbarButton';
import { metadataCache } from '../../../../components/sql-editor/metadata/metadataCache';
import { invalidateSchemaCache } from '../../../../lib/schemaCache';
import { useSchemaStore } from '../../../../stores/schemaStore';
import { useI18n } from '../../../../hooks/useI18n';
import { tid } from '../../../../lib/tid';

export interface RefreshCompletionButtonProps {
  dbSessionId: string;
  database?: string | null;
  compact?: boolean;
  disabled?: boolean;
  onRefreshed: (message: string) => void;
}

export function RefreshCompletionButton({
  dbSessionId,
  database,
  compact,
  disabled,
  onRefreshed,
}: RefreshCompletionButtonProps) {
  const { t } = useI18n();
  const [busy, setBusy] = useState(false);

  const handleRefresh = useCallback(async () => {
    if (!dbSessionId || busy) return;
    setBusy(true);
    try {
      // Drop the underlying table-schema/DDL layer first: clearing only the
      // editor snapshot would let it repopulate from the same stale entries.
      invalidateSchemaCache(dbSessionId);
      metadataCache.invalidateSession(dbSessionId);
      if (database) {
        await useSchemaStore.getState().loadTables(database, dbSessionId);
      }
      onRefreshed(t('query.refreshCompletionDone'));
    } finally {
      setBusy(false);
    }
  }, [dbSessionId, database, busy, onRefreshed, t]);

  return (
    <ToolbarButton
      compact={compact}
      variant="ghost"
      label={t('query.refreshCompletion')}
      title={t('query.refreshCompletionTitle')}
      icon={<RefreshCw className={`h-3.5 w-3.5${busy ? ' animate-spin' : ''}`} />}
      onClick={() => void handleRefresh()}
      disabled={disabled || busy || !dbSessionId}
      {...tid('editor-refresh-completion-button')}
    />
  );
}
