import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import {
  AlertTriangle,
  ArrowRight,
  CheckCircle2,
  Clock,
  Loader2,
  MinusCircle,
  Pause,
  Play,
  PlusCircle,
  RefreshCcw,
  Trash2,
  XCircle,
} from 'lucide-react';
import { TitleBar } from '../../components/TitleBar';
import { HeaderControls } from '../../components/HeaderControls';
import { StatusBar } from '../../components/StatusBar';
import { Button } from '../../components/ui/Button';
import { Select } from '../../components/ui/Select';
import { Dialog } from '../../components/ui/Dialog';
import { useThemeListener } from '../../hooks/useThemeListener';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';
import type { ConnectionConfig } from '../../types';

interface TableComparison {
  table: string;
  status: 'identical' | 'different' | 'source_only' | 'target_only';
  sourceRows: number | null;
  targetRows: number | null;
}

interface SyncTask {
  id: string;
  sourceConnectionId: string;
  targetConnectionId: string;
  sourceConfigId: string;
  targetConfigId: string;
  tables: string[];
  completedTables: string[];
  currentTable: string | null;
  currentTableOffset: number;
  sourceRowCounts: Record<string, number>;
  strategy: string;
  status: string;
  errorMessage: string | null;
  createdAt: string;
  updatedAt: string;
}

interface SyncProgress {
  taskId: string;
  phase: string;
  tableIndex: number;
  totalTables: number;
  currentTable: string;
  sourceRowCount: number;
  syncedRows: number;
  completedTables: string[];
  error: string | null;
}

interface ConflictInfo {
  table: string;
  originalRows: number;
  currentRows: number;
}

type SyncState = 'idle' | 'comparing' | 'compared' | 'syncing' | 'done';

function formatDuration(ms: number): string {
  const secs = Math.floor(ms / 1000);
  if (secs < 60) return `${secs}s`;
  const mins = Math.floor(secs / 60);
  const rem = secs % 60;
  return `${mins}m ${rem}s`;
}

export function DataSyncWindow() {
  useThemeListener();
  const { t } = useI18n();

  const [connections, setConnections] = useState<ConnectionConfig[]>([]);
  const [activeConns, setActiveConns] = useState<Record<string, string>>({});
  const [sourceId, setSourceId] = useState('');
  const [targetId, setTargetId] = useState('');
  const [comparisons, setComparisons] = useState<TableComparison[]>([]);
  const [selectedTables, setSelectedTables] = useState<Set<string>>(new Set());
  const [syncState, setSyncState] = useState<SyncState>('idle');
  const [errorMsg, setErrorMsg] = useState('');
  const [errorOpen, setErrorOpen] = useState(false);

  // Progress
  const [progress, setProgress] = useState<SyncProgress | null>(null);
  const [progressOpen, setProgressOpen] = useState(false);
  const [syncStartTime, setSyncStartTime] = useState(0);
  const [elapsed, setElapsed] = useState(0);

  // Resume
  const [savedTasks, setSavedTasks] = useState<SyncTask[]>([]);
  const [resumeDialogOpen, setResumeDialogOpen] = useState(false);
  const [resumeTask, setResumeTask] = useState<SyncTask | null>(null);
  const [conflicts, setConflicts] = useState<ConflictInfo[]>([]);
  const [conflictDialogOpen, setConflictDialogOpen] = useState(false);

  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  useEffect(() => {
    (async () => {
      try {
        const conns = await invoke<ConnectionConfig[]>('get_connections');
        setConnections(conns);
        const tasks = await invoke<SyncTask[]>('get_sync_tasks');
        setSavedTasks(tasks.filter((t) => t.status !== 'completed'));
      } catch (e) {
        console.error('Failed to load', e);
      }
    })();
  }, []);

  // Listen for sync progress events
  useEffect(() => {
    const unlisten = listen<SyncProgress>('sync:progress', (event) => {
      setProgress(event.payload);
      if (event.payload.phase === 'done') {
        setSyncState('done');
        if (timerRef.current) clearInterval(timerRef.current);
      } else if (event.payload.phase === 'error') {
        if (timerRef.current) clearInterval(timerRef.current);
      }
    });
    return () => { unlisten.then((fn) => fn()); };
  }, []);

  // Elapsed timer
  useEffect(() => {
    if (syncState === 'syncing' && syncStartTime > 0) {
      timerRef.current = setInterval(() => {
        setElapsed(Date.now() - syncStartTime);
      }, 500);
      return () => { if (timerRef.current) clearInterval(timerRef.current); };
    }
  }, [syncState, syncStartTime]);

  const connOptions = useMemo(() => {
    return connections.map((c) => ({
      value: c.id,
      label: `${c.name} (${c.databaseType})`,
    }));
  }, [connections]);

  const ensureConnected = useCallback(async (configId: string): Promise<string | null> => {
    if (activeConns[configId]) return activeConns[configId];
    try {
      const connectionId = await invoke<string>('connect', { configId });
      setActiveConns((prev) => ({ ...prev, [configId]: connectionId }));
      return connectionId;
    } catch (e) {
      setErrorMsg(`${t('sync.connectFailed')} ${e instanceof Error ? e.message : String(e)}`);
      setErrorOpen(true);
      return null;
    }
  }, [activeConns, t]);

  const handleCompare = useCallback(async () => {
    if (!sourceId || !targetId) {
      setErrorMsg(t('sync.selectBoth'));
      setErrorOpen(true);
      return;
    }
    if (sourceId === targetId) {
      setErrorMsg(t('sync.cannotSame'));
      setErrorOpen(true);
      return;
    }

    setSyncState('comparing');
    setComparisons([]);
    setSelectedTables(new Set());

    try {
      const srcConnId = await ensureConnected(sourceId);
      const tgtConnId = await ensureConnected(targetId);
      if (!srcConnId || !tgtConnId) {
        setSyncState('idle');
        return;
      }

      const results = await invoke<TableComparison[]>('compare_databases', {
        sourceConnectionId: srcConnId,
        targetConnectionId: tgtConnId,
      });
      setComparisons(results);
      const autoSelect = new Set(
        results
          .filter((r) => r.status !== 'identical' && r.status !== 'target_only')
          .map((r) => r.table),
      );
      setSelectedTables(autoSelect);
      setSyncState('compared');
    } catch (e) {
      setErrorMsg(e instanceof Error ? e.message : String(e));
      setErrorOpen(true);
      setSyncState('idle');
    }
  }, [sourceId, targetId, ensureConnected, t]);

  const startSync = useCallback(async (
    tablesToSync: string[],
    skipTables: string[] = [],
    strategy: string = 'full',
  ) => {
    const srcConnId = activeConns[sourceId];
    const tgtConnId = activeConns[targetId];
    if (!srcConnId || !tgtConnId) return;

    const taskId = crypto.randomUUID();
    setSyncState('syncing');
    setProgress(null);
    setProgressOpen(true);
    setSyncStartTime(Date.now());
    setElapsed(0);

    try {
      await invoke('sync_tables', {
        taskId,
        sourceConnectionId: srcConnId,
        targetConnectionId: tgtConnId,
        sourceConfigId: sourceId,
        targetConfigId: targetId,
        tables: tablesToSync,
        skipTables,
        strategy,
      });
      // Refresh saved tasks
      const tasks = await invoke<SyncTask[]>('get_sync_tasks');
      setSavedTasks(tasks.filter((t) => t.status !== 'completed'));
    } catch (e) {
      setErrorMsg(e instanceof Error ? e.message : String(e));
      setErrorOpen(true);
      setSyncState('compared');
    }
  }, [sourceId, targetId, activeConns]);

  const handleSync = useCallback(async () => {
    if (selectedTables.size === 0) return;
    await startSync(Array.from(selectedTables));
  }, [selectedTables, startSync]);

  const handleResumeClick = useCallback(async (task: SyncTask) => {
    setResumeTask(task);

    // Connect to source & target
    const srcConnId = await ensureConnected(task.sourceConfigId);
    const tgtConnId = await ensureConnected(task.targetConfigId);
    if (!srcConnId || !tgtConnId) return;

    setActiveConns((prev) => ({
      ...prev,
      [task.sourceConfigId]: srcConnId,
      [task.targetConfigId]: tgtConnId,
    }));
    setSourceId(task.sourceConfigId);
    setTargetId(task.targetConfigId);

    // Check for conflicts
    try {
      const result = await invoke<{ hasConflicts: boolean; conflicts: ConflictInfo[] }>(
        'check_sync_conflicts',
        { taskId: task.id },
      );

      if (result.hasConflicts) {
        setConflicts(result.conflicts);
        setConflictDialogOpen(true);
      } else {
        setResumeDialogOpen(true);
      }
    } catch (e) {
      setErrorMsg(`${t('sync.checkConflictFailed')} ${e instanceof Error ? e.message : String(e)}`);
      setErrorOpen(true);
    }
  }, [ensureConnected, t]);

  const handleResumeConfirm = useCallback(async (restartFromZero: boolean) => {
    setResumeDialogOpen(false);
    setConflictDialogOpen(false);
    if (!resumeTask) return;

    const srcConnId = activeConns[resumeTask.sourceConfigId];
    const tgtConnId = activeConns[resumeTask.targetConfigId];
    if (!srcConnId || !tgtConnId) return;

    // Delete old task
    await invoke('delete_sync_task', { taskId: resumeTask.id });

    const skip = restartFromZero ? [] : resumeTask.completedTables;
    const strategy = restartFromZero ? 'full' : 'continue';

    const taskId = crypto.randomUUID();
    setSyncState('syncing');
    setProgress(null);
    setProgressOpen(true);
    setSyncStartTime(Date.now());
    setElapsed(0);

    try {
      await invoke('sync_tables', {
        taskId,
        sourceConnectionId: srcConnId,
        targetConnectionId: tgtConnId,
        sourceConfigId: resumeTask.sourceConfigId,
        targetConfigId: resumeTask.targetConfigId,
        tables: resumeTask.tables,
        skipTables: skip,
        strategy,
      });
      const tasks = await invoke<SyncTask[]>('get_sync_tasks');
      setSavedTasks(tasks.filter((t) => t.status !== 'completed'));
    } catch (e) {
      setErrorMsg(e instanceof Error ? e.message : String(e));
      setErrorOpen(true);
    }
  }, [resumeTask, activeConns]);

  const handleDeleteTask = useCallback(async (taskId: string) => {
    try {
      await invoke('delete_sync_task', { taskId });
      setSavedTasks((prev) => prev.filter((t) => t.id !== taskId));
    } catch (e) {
      setErrorMsg(e instanceof Error ? e.message : String(e));
      setErrorOpen(true);
    }
  }, []);

  const toggleTable = useCallback((table: string) => {
    setSelectedTables((prev) => {
      const next = new Set(prev);
      if (next.has(table)) next.delete(table);
      else next.add(table);
      return next;
    });
  }, []);

  const selectAll = useCallback(() => {
    setSelectedTables(new Set(comparisons.filter((r) => r.status !== 'target_only').map((r) => r.table)));
  }, [comparisons]);

  const deselectAll = useCallback(() => {
    setSelectedTables(new Set());
  }, []);

  const statusIcon = (status: string) => {
    switch (status) {
      case 'identical': return <CheckCircle2 className="h-4 w-4 text-green-500" />;
      case 'different': return <RefreshCcw className="h-4 w-4 text-amber-500" />;
      case 'source_only': return <PlusCircle className="h-4 w-4 text-blue-500" />;
      case 'target_only': return <MinusCircle className="h-4 w-4 text-red-400" />;
      default: return null;
    }
  };

  const statusLabel = (status: string) => {
    switch (status) {
      case 'identical': return t('sync.identical');
      case 'different': return t('sync.different');
      case 'source_only': return t('sync.sourceOnly');
      case 'target_only': return t('sync.targetOnly');
      default: return status;
    }
  };

  const tableProgress = progress
    ? (progress.sourceRowCount > 0
      ? Math.round((progress.syncedRows / progress.sourceRowCount) * 100)
      : 0)
    : 0;

  const overallProgress = progress
    ? Math.round(((progress.completedTables.length + (progress.sourceRowCount > 0 ? progress.syncedRows / progress.sourceRowCount : 0)) / progress.totalTables) * 100)
    : 0;

  return (
    <div className="flex h-screen min-h-0 flex-col bg-surface text-fg">
      {/* Title bar */}
      <TitleBar title={t('sync.windowTitle')} rightContent={<HeaderControls />} />

      {/* Saved tasks banner */}
      {savedTasks.length > 0 && syncState !== 'syncing' && (
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
                  <Button variant="ghost" className="h-7 gap-1 px-2 text-xs" onClick={() => void handleResumeClick(task)}>
                    <Play className="h-3 w-3" /> {t('sync.continue')}
                  </Button>
                  <Button variant="ghost" className="h-7 px-2 text-xs text-red-500 hover:text-red-600" onClick={() => void handleDeleteTask(task.id)}>
                    <Trash2 className="h-3 w-3" />
                  </Button>
                </div>
              );
            })}
          </div>
        </div>
      )}

      {/* Connection selectors */}
      <div className="flex shrink-0 items-center gap-4 border-b border-edge px-6 py-4">
        <div className="min-w-0 flex-1">
          <label className="mb-1 block text-[11px] font-semibold uppercase tracking-wider text-fg-muted">{t('sync.source')}</label>
          <Select value={sourceId} options={connOptions} onChange={setSourceId} placeholder={t('sync.selectSource')} />
        </div>
        <ArrowRight className="mt-5 h-5 w-5 shrink-0 text-fg-muted" />
        <div className="min-w-0 flex-1">
          <label className="mb-1 block text-[11px] font-semibold uppercase tracking-wider text-fg-muted">{t('sync.target')}</label>
          <Select value={targetId} options={connOptions} onChange={setTargetId} placeholder={t('sync.selectTarget')} />
        </div>
        <div className="mt-5 shrink-0">
          <Button variant="primary" onClick={() => void handleCompare()} disabled={syncState === 'comparing' || syncState === 'syncing'}>
            {syncState === 'comparing' ? <Loader2 className="h-4 w-4 animate-spin" /> : <RefreshCcw className="h-4 w-4" />}
            {t('sync.compare')}
          </Button>
        </div>
      </div>

      {/* Table list */}
      <div className="min-h-0 flex-1 overflow-auto">
        {syncState === 'idle' && (
          <div className="flex h-full items-center justify-center text-sm text-fg-muted">
            {t('sync.selectPrompt')}
          </div>
        )}

        {syncState === 'comparing' && (
          <div className="flex h-full items-center justify-center gap-2 text-sm text-fg-muted">
            <Loader2 className="h-4 w-4 animate-spin" />
            {t('sync.comparing')}
          </div>
        )}

        {(syncState === 'compared' || syncState === 'syncing' || syncState === 'done') && (
          <div className="p-4">
            <div className="mb-3 flex items-center gap-2">
              <Button variant="ghost" className="text-xs" onClick={selectAll}>{t('common.selectAll')}</Button>
              <Button variant="ghost" className="text-xs" onClick={deselectAll}>{t('common.deselectAll')}</Button>
              <div className="flex-1" />
              <span className="text-xs text-fg-muted">
                {comparisons.filter((r) => r.status === 'identical').length} {t('sync.identical')} /
                {' '}{comparisons.filter((r) => r.status === 'different').length} {t('sync.different')} /
                {' '}{comparisons.filter((r) => r.status === 'source_only').length} {t('sync.sourceOnly')} /
                {' '}{comparisons.filter((r) => r.status === 'target_only').length} {t('sync.targetOnly')}
              </span>
            </div>

            <div className="flex items-center gap-3 rounded-t-lg border border-edge bg-surface-alt px-3 py-2 text-[11px] font-semibold uppercase tracking-wider text-fg-muted">
              <div className="w-6" />
              <div className="w-6" />
              <div className="min-w-0 flex-1">{t('sync.tableName')}</div>
              <div className="w-20 text-right">{t('sync.sourceRows')}</div>
              <div className="w-20 text-right">{t('sync.targetRows')}</div>
              <div className="w-20 text-center">{t('sync.status')}</div>
            </div>

            {comparisons.map((row) => {
              const isSelected = selectedTables.has(row.table);
              const disabled = row.status === 'target_only';
              return (
                <div
                  key={row.table}
                  className={cn(
                    'flex items-center gap-3 border-x border-b border-edge px-3 py-2 text-[13px] transition-colors',
                    isSelected && !disabled && 'bg-blue-500/5',
                    !disabled && 'cursor-pointer hover:bg-surface-raised/50',
                    disabled && 'opacity-50',
                  )}
                  onClick={() => !disabled && toggleTable(row.table)}
                >
                  <div className="w-6 shrink-0">
                    <input type="checkbox" checked={isSelected} disabled={disabled} onChange={() => toggleTable(row.table)} className="h-3.5 w-3.5 rounded border-edge" />
                  </div>
                  <div className="w-6 shrink-0">{statusIcon(row.status)}</div>
                  <div className="min-w-0 flex-1 truncate font-mono text-fg">{row.table}</div>
                  <div className="w-20 text-right tabular-nums text-fg-secondary">{row.sourceRows ?? '-'}</div>
                  <div className="w-20 text-right tabular-nums text-fg-secondary">{row.targetRows ?? '-'}</div>
                  <div className="w-20 text-center text-xs text-fg-muted">{statusLabel(row.status)}</div>
                </div>
              );
            })}
          </div>
        )}
      </div>

      {/* Footer */}
      {(syncState === 'compared' || syncState === 'syncing' || syncState === 'done') && (
        <div className="flex shrink-0 items-center gap-3 border-t border-edge px-6 py-3">
          <span className="text-xs text-fg-muted">{t('sync.selected', { selected: selectedTables.size, total: comparisons.length })}</span>
          <div className="flex-1" />
          <Button variant="primary" onClick={() => void handleSync()} disabled={syncState === 'syncing' || selectedTables.size === 0}>
            {syncState === 'syncing' ? <Loader2 className="h-4 w-4 animate-spin" /> : <RefreshCcw className="h-4 w-4" />}
            {syncState === 'done' ? t('sync.reSync') : t('sync.startSync')}
          </Button>
        </div>
      )}

      <StatusBar
        left={<span className="truncate">{t('sync.title')}</span>}
        right={<span className="tabular-nums">DataZen v0.8.0</span>}
      />

      {/* ── Progress Dialog ── */}
      <Dialog
        open={progressOpen}
        title={t('sync.progressTitle')}
        onClose={() => { if (progress?.phase === 'done' || progress?.phase === 'error') setProgressOpen(false); }}
        className="max-w-lg"
        footer={
          (progress?.phase === 'done' || progress?.phase === 'error') ? (
            <Button variant="primary" onClick={() => setProgressOpen(false)}>{t('common.close')}</Button>
          ) : undefined
        }
      >
        <div className="space-y-4">
          {/* Overall progress */}
          <div>
            <div className="mb-1.5 flex items-center justify-between text-xs text-fg-muted">
              <span>{t('sync.overallProgress')}</span>
              <span>{t('sync.tableCount', { done: progress?.completedTables.length ?? 0, total: progress?.totalTables ?? 0 })}</span>
            </div>
            <div className="h-2.5 overflow-hidden rounded-full bg-surface-raised">
              <div
                className={cn(
                  'h-full rounded-full transition-all duration-300',
                  progress?.phase === 'error' ? 'bg-red-500' : progress?.phase === 'done' ? 'bg-green-500' : 'bg-blue-500',
                )}
                style={{ width: `${Math.min(overallProgress, 100)}%` }}
              />
            </div>
          </div>

          {/* Current table */}
          {progress && progress.phase === 'syncing' && (
            <div className="rounded-lg border border-edge bg-surface-alt p-3">
              <div className="mb-1 flex items-center justify-between">
                <span className="truncate font-mono text-sm text-fg">{progress.currentTable}</span>
                <span className="ml-2 shrink-0 text-xs tabular-nums text-fg-muted">
                  {t('sync.rowProgress', { synced: progress.syncedRows.toLocaleString(), total: progress.sourceRowCount.toLocaleString() })}
                </span>
              </div>
              <div className="h-1.5 overflow-hidden rounded-full bg-surface-raised">
                <div className="h-full rounded-full bg-blue-400 transition-all duration-200" style={{ width: `${Math.min(tableProgress, 100)}%` }} />
              </div>
            </div>
          )}

          {/* Phase-specific info */}
          {progress?.phase === 'counting' && (
            <div className="flex items-center gap-2 text-sm text-fg-muted">
              <Loader2 className="h-4 w-4 animate-spin" />
              {t('sync.countingRows')}
            </div>
          )}

          {progress?.phase === 'done' && (
            <div className="flex items-center gap-2 text-sm text-green-500">
              <CheckCircle2 className="h-4 w-4" />
              {t('sync.syncDone', { count: progress.completedTables.length })}
            </div>
          )}

          {progress?.phase === 'error' && (
            <div className="flex items-start gap-2 text-sm text-red-500">
              <XCircle className="mt-0.5 h-4 w-4 shrink-0" />
              <span className="break-all">{progress.error}</span>
            </div>
          )}

          {/* Elapsed time */}
          <div className="flex items-center gap-1.5 text-xs text-fg-muted">
            <Clock className="h-3.5 w-3.5" />
            {t('sync.elapsed')} {formatDuration(elapsed)}
          </div>

          {/* Completed tables list */}
          {progress && progress.completedTables.length > 0 && (
            <details className="text-xs">
              <summary className="cursor-pointer text-fg-muted hover:text-fg">
                {t('sync.completedTables')} ({progress.completedTables.length})
              </summary>
              <div className="mt-1 max-h-32 overflow-auto rounded border border-edge bg-surface p-2 font-mono text-fg-secondary">
                {progress.completedTables.map((tableName) => (
                  <div key={tableName} className="flex items-center gap-1">
                    <CheckCircle2 className="h-3 w-3 text-green-500" /> {tableName}
                  </div>
                ))}
              </div>
            </details>
          )}
        </div>
      </Dialog>

      {/* ── Resume Dialog (no conflicts) ── */}
      <Dialog
        open={resumeDialogOpen}
        title={t('sync.resumeTitle')}
        description={resumeTask ? t('sync.resumeDesc', { done: resumeTask.completedTables.length, total: resumeTask.tables.length }) : ''}
        onClose={() => setResumeDialogOpen(false)}
        footer={
          <>
            <Button variant="ghost" onClick={() => setResumeDialogOpen(false)}>{t('common.cancel')}</Button>
            <Button variant="ghost" onClick={() => void handleResumeConfirm(true)}>
              <RefreshCcw className="h-3.5 w-3.5" /> {t('sync.resumeRestart')}
            </Button>
            <Button variant="primary" onClick={() => void handleResumeConfirm(false)}>
              <Play className="h-3.5 w-3.5" /> {t('sync.resumeContinue')}
            </Button>
          </>
        }
      >
        <div className="space-y-3 text-sm text-fg-secondary">
          <p>{t('sync.resumeExplain')}</p>
          <div className="rounded-lg border border-edge bg-surface p-3 text-xs">
            <div><span className="font-semibold text-fg">{t('sync.resumeContinue')}</span>：{t('sync.resumeContinueDesc', { count: resumeTask?.completedTables.length ?? 0 })}</div>
            <div className="mt-2"><span className="font-semibold text-fg">{t('sync.resumeRestart')}</span>：{t('sync.resumeRestartDesc', { count: resumeTask?.tables.length ?? 0 })}</div>
          </div>
        </div>
      </Dialog>

      {/* ── Conflict Dialog ── */}
      <Dialog
        open={conflictDialogOpen}
        title={t('sync.conflictTitle')}
        description={t('sync.conflictDesc')}
        onClose={() => setConflictDialogOpen(false)}
        footer={
          <>
            <Button variant="ghost" onClick={() => setConflictDialogOpen(false)}>{t('common.cancel')}</Button>
            <Button variant="ghost" onClick={() => void handleResumeConfirm(true)}>
              <RefreshCcw className="h-3.5 w-3.5" /> {t('sync.resumeRestart')}
            </Button>
            <Button variant="primary" onClick={() => void handleResumeConfirm(false)}>
              <Play className="h-3.5 w-3.5" /> {t('sync.conflictContinue')}
            </Button>
          </>
        }
      >
        <div className="space-y-3 text-sm">
          <div className="flex items-start gap-2 rounded-lg border border-amber-300/30 bg-amber-500/10 p-3 text-xs text-amber-700 dark:text-amber-300">
            <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
            <span>{t('sync.conflictWarning')}</span>
          </div>

          <div className="overflow-hidden rounded-lg border border-edge">
            <div className="flex items-center gap-3 bg-surface-alt px-3 py-1.5 text-[11px] font-semibold uppercase tracking-wider text-fg-muted">
              <div className="min-w-0 flex-1">{t('sync.tableName')}</div>
              <div className="w-24 text-right">{t('sync.originalRows')}</div>
              <div className="w-24 text-right">{t('sync.currentRows')}</div>
            </div>
            {conflicts.map((c) => (
              <div key={c.table} className="flex items-center gap-3 border-t border-edge px-3 py-1.5 text-xs">
                <div className="min-w-0 flex-1 truncate font-mono text-fg">{c.table}</div>
                <div className="w-24 text-right tabular-nums text-fg-secondary">{c.originalRows.toLocaleString()}</div>
                <div className="w-24 text-right tabular-nums text-amber-600 dark:text-amber-400">{c.currentRows.toLocaleString()}</div>
              </div>
            ))}
          </div>

          <div className="rounded-lg border border-edge bg-surface p-3 text-xs text-fg-muted">
            <div><span className="font-semibold text-fg">{t('sync.conflictContinue')}</span>：{t('sync.conflictContinueDesc')}</div>
            <div className="mt-1"><span className="font-semibold text-fg">{t('sync.resumeRestart')}</span>：{t('sync.conflictRestartDesc')}</div>
          </div>
        </div>
      </Dialog>

      {/* ── Error Dialog ── */}
      <Dialog
        open={errorOpen}
        title={t('common.hint')}
        onClose={() => setErrorOpen(false)}
        footer={<Button variant="primary" onClick={() => setErrorOpen(false)}>{t('common.ok')}</Button>}
      >
        <p className="whitespace-pre-wrap break-all text-sm text-fg-secondary">{errorMsg}</p>
      </Dialog>
    </div>
  );
}
