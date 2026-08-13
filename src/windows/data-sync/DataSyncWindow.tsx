import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import {
  ArrowRight,
  CheckCircle2,
  ChevronDown,
  ChevronRight,
  Code2,
  Loader2,
  MinusCircle,
  PlusCircle,
  RefreshCcw,
  X,
} from 'lucide-react';
import { TitleBar } from '../../components/TitleBar';
import { StatusBar } from '../../components/StatusBar';
import { Button } from '../../components/ui/Button';
import { Select } from '../../components/ui/Select';
import { Dialog } from '../../components/ui/Dialog';
import { SchemaDiffPanel } from '../../components/schema/SchemaDiffPanel';
import { syncCommands, type SyncTask } from '../../commands/sync';
import { useThemeListener } from '../../hooks/useThemeListener';
import { useI18n } from '../../hooks/useI18n';
import { useSettingsStore } from '../../stores/settingsStore';
import { cn } from '../../lib/cn';
import { resolveSyncPairing } from '../../lib/syncPairing';
import type {
  ConnectionConfig,
  RowMismatch,
  TableComparison,
  TableDataCompare,
  TableSchemaDiff,
} from '../../types';
import { SyncProgressPanel } from './SyncProgressPanel';
import { ConflictSyncDialog, ResumeSyncDialog } from './ResumeSyncDialog';
import type { ConflictInfo, SyncProgress, SyncState } from './utils';

export function DataSyncWindow() {
  useThemeListener();
  const { t } = useI18n();
  const loadSettings = useSettingsStore((s) => s.loadSettings);

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

  // Detail panel
  const [detailTable, setDetailTable] = useState<string | null>(null);
  const [schemaDiff, setSchemaDiff] = useState<TableSchemaDiff | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);
  const [showDdl, setShowDdl] = useState(false);
  const [expandedTables, setExpandedTables] = useState<Set<string>>(new Set());
  const [dataCompareCache, setDataCompareCache] = useState<Record<string, TableDataCompare>>({});
  const [dataCompareLoading, setDataCompareLoading] = useState<Set<string>>(new Set());

  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  useEffect(() => { void loadSettings(); }, [loadSettings]);

  useEffect(() => {
    (async () => {
      try {
        const conns = await invoke<ConnectionConfig[]>('get_connections');
        setConnections(conns);
        const tasks = await syncCommands.getSyncTasks();
        setSavedTasks(tasks.filter((t) => t.status !== 'completed'));
      } catch (e) {
        console.error('Failed to load', e);
      }
    })();
  }, []);

  useEffect(() => {
    if (!sourceId || !targetId) return;
    const src = connections.find((c) => c.id === sourceId);
    const tgt = connections.find((c) => c.id === targetId);
    if (!src || !tgt) return;
    if (
      sourceId === targetId
      || !resolveSyncPairing(src.databaseType, tgt.databaseType).supported
    ) {
      setTargetId('');
    }
  }, [sourceId, connections, targetId]);

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

  const sourceConn = useMemo(
    () => connections.find((c) => c.id === sourceId),
    [connections, sourceId],
  );

  const connOptions = useMemo(() => {
    return connections.map((c) => ({
      value: c.id,
      label: `${c.name} (${c.databaseType})`,
    }));
  }, [connections]);

  const targetOptions = useMemo(() => {
    const hint = t('sync.unsupportedHint');
    const srcType = sourceConn?.databaseType;
    return connections.map((c) => {
      const sameConnection = c.id === sourceId;
      const unsupported = Boolean(
        srcType && !sameConnection && !resolveSyncPairing(srcType, c.databaseType).supported,
      );
      const disabled = sameConnection || unsupported;
      const base = `${c.name} (${c.databaseType})`;
      return {
        value: c.id,
        label: unsupported ? `${base} — ${hint}` : base,
        disabled,
        title: unsupported ? hint : undefined,
      };
    });
  }, [connections, sourceId, sourceConn?.databaseType, t]);

  const activePairing = useMemo(() => {
    if (!sourceConn || !targetId) return null;
    const tgt = connections.find((c) => c.id === targetId);
    if (!tgt) return null;
    return resolveSyncPairing(sourceConn.databaseType, tgt.databaseType);
  }, [connections, sourceConn, targetId]);

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
    setDetailTable(null);
    setSchemaDiff(null);
    setExpandedTables(new Set());
    setDataCompareCache({});

    try {
      const srcConnId = await ensureConnected(sourceId);
      const tgtConnId = await ensureConnected(targetId);
      if (!srcConnId || !tgtConnId) {
        setSyncState('idle');
        return;
      }

      const results = await syncCommands.compareDatabases(srcConnId, tgtConnId);
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
    const resumeTable =
      !restartFromZero && resumeTask.currentTable && resumeTask.currentTableOffset > 0
        ? resumeTask.currentTable
        : null;
    const resumeOffset =
      !restartFromZero && resumeTask.currentTable && resumeTask.currentTableOffset > 0
        ? resumeTask.currentTableOffset
        : 0;

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
        resumeTable,
        resumeOffset,
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

  const openTableDetail = useCallback(async (table: string) => {
    const srcConnId = activeConns[sourceId];
    const tgtConnId = activeConns[targetId];
    if (!srcConnId || !tgtConnId) return;

    setDetailTable(table);
    setSchemaDiff(null);
    setShowDdl(false);
    setDetailLoading(true);

    try {
      const diff = await syncCommands.compareTableSchemas(srcConnId, tgtConnId, table);
      setSchemaDiff(diff);
    } catch (e) {
      setErrorMsg(e instanceof Error ? e.message : String(e));
      setErrorOpen(true);
      setDetailTable(null);
    } finally {
      setDetailLoading(false);
    }
  }, [activeConns, sourceId, targetId]);

  const toggleExpandTable = useCallback(async (table: string) => {
    const srcConnId = activeConns[sourceId];
    const tgtConnId = activeConns[targetId];
    if (!srcConnId || !tgtConnId) return;

    setExpandedTables((prev) => {
      const next = new Set(prev);
      if (next.has(table)) next.delete(table);
      else next.add(table);
      return next;
    });

    if (dataCompareCache[table] || dataCompareLoading.has(table)) return;

    setDataCompareLoading((prev) => new Set(prev).add(table));
    try {
      const result = await syncCommands.compareTableData(srcConnId, tgtConnId, table);
      setDataCompareCache((prev) => ({ ...prev, [table]: result }));
    } catch (e) {
      setErrorMsg(e instanceof Error ? e.message : String(e));
      setErrorOpen(true);
      setExpandedTables((prev) => {
        const next = new Set(prev);
        next.delete(table);
        return next;
      });
    } finally {
      setDataCompareLoading((prev) => {
        const next = new Set(prev);
        next.delete(table);
        return next;
      });
    }
  }, [activeConns, sourceId, targetId, dataCompareCache, dataCompareLoading]);

  const renderMismatchRows = (mismatches: RowMismatch[]) => {
    if (mismatches.length === 0) {
      return <div className="px-3 py-2 text-xs text-fg-muted">{t('sync.noMismatches')}</div>;
    }
    return mismatches.map((m) => (
      <div key={m.key} className="border-t border-edge px-3 py-2 text-xs">
        <div className="mb-1 flex items-center gap-2">
          <span className="rounded bg-surface-raised px-1.5 py-0.5 font-mono text-[10px] text-fg-muted">{m.key}</span>
          <span className="text-fg-secondary">{t(`sync.mismatch.${m.kind}`)}</span>
        </div>
        {m.source && (
          <div className="mb-1 font-mono text-[11px] text-blue-600 dark:text-blue-400">
            {t('sync.source')}: {JSON.stringify(m.source)}
          </div>
        )}
        {m.target && (
          <div className="font-mono text-[11px] text-amber-600 dark:text-amber-400">
            {t('sync.target')}: {JSON.stringify(m.target)}
          </div>
        )}
      </div>
    ));
  };

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

  return (
    <div className="flex h-screen min-h-0 flex-col bg-surface text-fg">
      {/* Title bar */}
      <TitleBar title={t('sync.windowTitle')} />

      <div
        data-testid="data-sync-overwrite-retired"
        className="shrink-0 border-b border-amber-500/40 bg-amber-500/10 px-6 py-2 text-xs text-fg-secondary"
      >
        {t('sync.overwriteRetiredBanner')}
      </div>

      {/* Connection selectors */}
      <div className="flex shrink-0 items-center gap-4 border-b border-edge px-6 py-4">
        <div className="min-w-0 flex-1">
          <label className="mb-1 block text-[11px] font-semibold uppercase tracking-wider text-fg-muted">{t('sync.source')}</label>
          <Select value={sourceId} options={connOptions} onChange={setSourceId} placeholder={t('sync.selectSource')} />
        </div>
        <ArrowRight className="mt-5 h-5 w-5 shrink-0 text-fg-muted" />
        <div className="min-w-0 flex-1">
          <label className="mb-1 block text-[11px] font-semibold uppercase tracking-wider text-fg-muted">{t('sync.target')}</label>
          <Select value={targetId} options={targetOptions} onChange={setTargetId} placeholder={t('sync.selectTarget')} />
        </div>
        <div className="mt-5 flex shrink-0 flex-col items-end gap-1">
          {activePairing?.supported && (
            <span className="text-[10px] font-medium uppercase tracking-wide text-fg-muted">
              {activePairing.path === 'direct' ? t('sync.pathDirect') : t('sync.pathIr')}
            </span>
          )}
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
          <div className="flex h-full min-h-0">
            <div className={cn('min-h-0 flex-1 overflow-auto p-4', detailTable && 'border-r border-edge')}>
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
              <div className="w-6" />
              <div className="min-w-0 flex-1">{t('sync.tableName')}</div>
              <div className="w-20 text-right">{t('sync.sourceRows')}</div>
              <div className="w-20 text-right">{t('sync.targetRows')}</div>
              <div className="w-20 text-center">{t('sync.status')}</div>
            </div>

            {comparisons.map((row) => {
              const isSelected = selectedTables.has(row.table);
              const disabled = row.status === 'target_only';
              const isExpanded = expandedTables.has(row.table);
              const canExpand = row.status === 'different';
              const isDetail = detailTable === row.table;
              const dataCompare = dataCompareCache[row.table];
              const isDataLoading = dataCompareLoading.has(row.table);

              return (
                <div key={row.table}>
                  <div
                    className={cn(
                      'flex items-center gap-3 border-x border-b border-edge px-3 py-2 text-[13px] transition-colors',
                      isSelected && !disabled && 'bg-blue-500/5',
                      isDetail && 'bg-surface-raised/70',
                      !disabled && 'hover:bg-surface-raised/50',
                      disabled && 'opacity-50',
                    )}
                  >
                    <div className="w-6 shrink-0">
                      {canExpand ? (
                        <button
                          type="button"
                          className="flex h-4 w-4 items-center justify-center text-fg-muted hover:text-fg"
                          onClick={(e) => {
                            e.stopPropagation();
                            if (!isExpanded) void toggleExpandTable(row.table);
                            else {
                              setExpandedTables((prev) => {
                                const next = new Set(prev);
                                next.delete(row.table);
                                return next;
                              });
                            }
                          }}
                        >
                          {isDataLoading ? (
                            <Loader2 className="h-3.5 w-3.5 animate-spin" />
                          ) : isExpanded ? (
                            <ChevronDown className="h-3.5 w-3.5" />
                          ) : (
                            <ChevronRight className="h-3.5 w-3.5" />
                          )}
                        </button>
                      ) : null}
                    </div>
                    <div className="w-6 shrink-0">
                      <input
                        type="checkbox"
                        checked={isSelected}
                        disabled={disabled}
                        onChange={() => toggleTable(row.table)}
                        onClick={(e) => e.stopPropagation()}
                        className="h-3.5 w-3.5 rounded border-edge"
                      />
                    </div>
                    <div className="w-6 shrink-0">{statusIcon(row.status)}</div>
                    <button
                      type="button"
                      className="min-w-0 flex-1 truncate text-left font-mono text-fg"
                      onClick={() => { if (!disabled) void openTableDetail(row.table); }}
                    >
                      {row.table}
                    </button>
                    <div className="w-20 text-right tabular-nums text-fg-secondary">{row.sourceRows ?? '-'}</div>
                    <div className="w-20 text-right tabular-nums text-fg-secondary">{row.targetRows ?? '-'}</div>
                    <div className="w-20 text-center text-xs text-fg-muted">{statusLabel(row.status)}</div>
                  </div>

                  {canExpand && isExpanded && (
                    <div className="border-x border-b border-edge bg-surface-alt/50">
                      {isDataLoading && (
                        <div className="flex items-center gap-2 px-4 py-3 text-xs text-fg-muted">
                          <Loader2 className="h-3.5 w-3.5 animate-spin" />
                          {t('sync.loadingDataCompare')}
                        </div>
                      )}
                      {dataCompare && (
                        <>
                          <div className="flex items-center gap-3 border-b border-edge px-3 py-1.5 text-[11px] text-fg-muted">
                            <span>{t('sync.sampledRows', { count: dataCompare.sampledRows })}</span>
                            <span>{t('sync.mismatchCount', { count: dataCompare.mismatches.length })}{dataCompare.truncated ? '+' : ''}</span>
                          </div>
                          {renderMismatchRows(dataCompare.mismatches)}
                        </>
                      )}
                    </div>
                  )}
                </div>
              );
            })}
            </div>

            {detailTable && (
              <div className="flex w-[380px] shrink-0 flex-col bg-surface">
                <div className="flex items-center gap-2 border-b border-edge px-3 py-2">
                  <span className="min-w-0 flex-1 truncate font-mono text-sm font-medium text-fg">{detailTable}</span>
                  {schemaDiff?.sourceDdl && (
                    <Button
                      variant="ghost"
                      className="h-7 px-2 text-xs"
                      onClick={() => setShowDdl((v) => !v)}
                    >
                      <Code2 className="h-3.5 w-3.5" />
                      {t('sync.showDdl')}
                    </Button>
                  )}
                  <Button
                    variant="ghost"
                    className="h-7 w-7 p-0"
                    onClick={() => { setDetailTable(null); setSchemaDiff(null); setShowDdl(false); }}
                  >
                    <X className="h-4 w-4" />
                  </Button>
                </div>

                <div className="min-h-0 flex-1 overflow-auto p-3">
                  {detailLoading && (
                    <div className="flex items-center gap-2 text-sm text-fg-muted">
                      <Loader2 className="h-4 w-4 animate-spin" />
                      {t('sync.loadingSchemaDiff')}
                    </div>
                  )}

                  {!detailLoading && schemaDiff && !showDdl && (
                    <SchemaDiffPanel diff={schemaDiff} />
                  )}

                  {!detailLoading && schemaDiff && showDdl && (
                    <div className="space-y-3 text-xs">
                      {schemaDiff.sourceDdl && (
                        <section>
                          <h4 className="mb-1 font-semibold text-fg">{t('sync.sourceDdl')}</h4>
                          <pre className="overflow-x-auto rounded border border-edge bg-surface-alt p-2 font-mono text-[10px] text-fg-secondary">{schemaDiff.sourceDdl}</pre>
                        </section>
                      )}
                      {schemaDiff.targetDdl && (
                        <section>
                          <h4 className="mb-1 font-semibold text-fg">{t('sync.targetDdl')}</h4>
                          <pre className="overflow-x-auto rounded border border-edge bg-surface-alt p-2 font-mono text-[10px] text-fg-secondary">{schemaDiff.targetDdl}</pre>
                        </section>
                      )}
                    </div>
                  )}
                </div>
              </div>
            )}
          </div>
        )}
      </div>

      {/* Footer */}
      {(syncState === 'compared' || syncState === 'syncing' || syncState === 'done') && (
        <div className="flex shrink-0 items-center gap-3 border-t border-edge px-6 py-3">
          <span className="text-xs text-fg-muted">{t('sync.selected', { selected: selectedTables.size, total: comparisons.length })}</span>
          <div className="flex-1" />
          <Button
            variant="primary"
            onClick={() => void handleSync()}
            disabled
            title={t('sync.applyUnavailable')}
            data-testid="data-sync-start-disabled"
          >
            {syncState === 'syncing' ? <Loader2 className="h-4 w-4 animate-spin" /> : <RefreshCcw className="h-4 w-4" />}
            {syncState === 'done' ? t('sync.reSync') : t('sync.startSync')}
          </Button>
        </div>
      )}

      <StatusBar
        left={<span className="truncate">{t('sync.title')}</span>}
        right={<span className="tabular-nums">DataZen v0.0.9</span>}
      />

      <SyncProgressPanel
        open={progressOpen}
        progress={progress}
        elapsed={elapsed}
        onClose={() => setProgressOpen(false)}
      />

      <ResumeSyncDialog
        open={resumeDialogOpen}
        resumeTask={resumeTask}
        onClose={() => setResumeDialogOpen(false)}
        onConfirm={handleResumeConfirm}
      />

      <ConflictSyncDialog
        open={conflictDialogOpen}
        conflicts={conflicts}
        onClose={() => setConflictDialogOpen(false)}
        onConfirm={handleResumeConfirm}
      />

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
