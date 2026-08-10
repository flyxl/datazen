import { Pause, Play, Trash2 } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { useI18n } from '../../hooks/useI18n';
import type { ConnectionConfig } from '../../types';
import type { SyncTask } from '../../commands/sync';
import type { SyncState } from './utils';

interface SavedTasksBannerProps {
  savedTasks: SyncTask[];
  syncState: SyncState;
  connections: ConnectionConfig[];
  onResume: (task: SyncTask) => void;
  onDelete: (taskId: string) => void;
}

export function SavedTasksBanner({
  savedTasks,
  syncState,
  connections,
  onResume,
  onDelete,
}: SavedTasksBannerProps) {
  const { t } = useI18n();

  if (savedTasks.length === 0 || syncState === 'syncing') {
    return null;
  }

  return (
    <div className="border-b border-edge bg-amber-500/5 px-6 py-3">
      <div className="mb-2 flex items-center gap-2 text-xs font-semibold text-amber-600 dark:text-amber-400">
        <Pause className="h-3.5 w-3.5" />
        {t('sync.savedTasks', { count: savedTasks.length })}
      </div>
      <div className="space-y-2">
        {savedTasks.map((task) => {
          const srcName = connections.find((c) => c.id === task.sourceConfigId)?.name ?? task.sourceConfigId;
          const tgtName = connections.find((c) => c.id === task.targetConfigId)?.name ?? task.targetConfigId;
          return (
            <div key={task.id} className="flex items-center gap-3 rounded-lg border border-edge bg-surface px-3 py-2 text-xs">
              <div className="min-w-0 flex-1">
                <span className="font-medium text-fg">{srcName}</span>
                <span className="mx-1 text-fg-muted">→</span>
                <span className="font-medium text-fg">{tgtName}</span>
                <span className="ml-2 text-fg-muted">
                  ({t('sync.tablesCompleted', { done: task.completedTables.length, total: task.tables.length })})
                </span>
                {task.status === 'failed' && task.errorMessage && (
                  <span className="ml-2 text-red-500">{t('sync.failedMsg')} {task.errorMessage.slice(0, 60)}…</span>
                )}
              </div>
              <Button variant="ghost" className="h-7 gap-1 px-2 text-xs" onClick={() => void onResume(task)}>
                <Play className="h-3 w-3" /> {t('sync.continue')}
              </Button>
              <Button variant="ghost" className="h-7 px-2 text-xs text-red-500 hover:text-red-600" onClick={() => void onDelete(task.id)}>
                <Trash2 className="h-3 w-3" />
              </Button>
            </div>
          );
        })}
      </div>
    </div>
  );
}
