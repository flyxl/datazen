import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { ArrowRight, Loader2, RefreshCcw } from 'lucide-react';
import { TitleBar } from '../../components/TitleBar';
import { StatusBar } from '../../components/StatusBar';
import { Button } from '../../components/ui/Button';
import { Select } from '../../components/ui/Select';
import { Dialog } from '../../components/ui/Dialog';
import { syncCommands } from '../../commands/sync';
import { databaseCommands } from '../../commands/database';
import { useI18n } from '../../hooks/useI18n';
import { useSettingsStore } from '../../stores/settingsStore';
import { cn } from '../../lib/cn';
import { resolveSyncPairing } from '../../lib/syncPairing';
import type { ConnectionConfig } from '../../types';
import type { SyncState } from './utils';
import {
  displayTableName,
  mappingLabelKey,
  rowDiffCounts,
  summarizeMappings,
  tableHasRowDiffs,
  type DataSyncTableResult,
} from './mappingView';

export function DataSyncWindow() {
  const { t } = useI18n();
  const loadSettings = useSettingsStore((s) => s.loadSettings);

  const [connections, setConnections] = useState<ConnectionConfig[]>([]);
  const [activeConns, setActiveConns] = useState<Record<string, string>>({});
  const [sourceId, setSourceId] = useState('');
  const [targetId, setTargetId] = useState('');
  const [sourceDatabases, setSourceDatabases] = useState<string[]>([]);
  const [targetDatabases, setTargetDatabases] = useState<string[]>([]);
  const [sourceDatabase, setSourceDatabase] = useState('');
  const [targetDatabase, setTargetDatabase] = useState('');
  const [mappingResults, setMappingResults] = useState<DataSyncTableResult[]>([]);
  const [selectedTables, setSelectedTables] = useState<Set<string>>(new Set());
  const [syncState, setSyncState] = useState<SyncState>('idle');
  const [errorMsg, setErrorMsg] = useState('');
  const [errorOpen, setErrorOpen] = useState(false);
  const jobIdRef = useRef<string | null>(null);

  useEffect(() => {
    void loadSettings();
  }, [loadSettings]);

  useEffect(() => {
    (async () => {
      try {
        const conns = await invoke<ConnectionConfig[]>('get_connections');
        setConnections(conns);
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
      sourceId === targetId ||
      !resolveSyncPairing(src.databaseType, tgt.databaseType).supported
    ) {
      setTargetId('');
    }
  }, [sourceId, connections, targetId]);

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

  const ensureConnected = useCallback(
    async (configId: string): Promise<string | null> => {
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
    },
    [activeConns, t],
  );

  // Load the source connection's databases when it is selected.
  useEffect(() => {
    if (!sourceId) return;
    let cancelled = false;
    const cfg = connections.find((c) => c.id === sourceId);
    (async () => {
      try {
        const connId = await ensureConnected(sourceId);
        if (!connId || cancelled) return;
        const dbs = await databaseCommands.getDatabases(connId);
        if (cancelled) return;
        setSourceDatabases(dbs);
        const preferred = cfg?.database ?? '';
        setSourceDatabase((prev) =>
          dbs.includes(preferred) ? preferred : prev && dbs.includes(prev) ? prev : (dbs[0] ?? ''),
        );
      } catch {
        if (!cancelled) setSourceDatabases([]);
      }
    })();
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sourceId, connections, ensureConnected]);

  // Load the target connection's databases when it is selected.
  useEffect(() => {
    if (!targetId) return;
    let cancelled = false;
    const cfg = connections.find((c) => c.id === targetId);
    (async () => {
      try {
        const connId = await ensureConnected(targetId);
        if (!connId || cancelled) return;
        const dbs = await databaseCommands.getDatabases(connId);
        if (cancelled) return;
        setTargetDatabases(dbs);
        const preferred = cfg?.database ?? '';
        setTargetDatabase((prev) =>
          dbs.includes(preferred) ? preferred : prev && dbs.includes(prev) ? prev : (dbs[0] ?? ''),
        );
      } catch {
        if (!cancelled) setTargetDatabases([]);
      }
    })();
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [targetId, connections, ensureConnected]);

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
    if (!sourceDatabase || !targetDatabase) {
      setErrorMsg(t('sync.selectDbRequired'));
      setErrorOpen(true);
      return;
    }

    setSyncState('comparing');
    setSelectedTables(new Set());
    setMappingResults([]);
    const jobId = crypto.randomUUID();
    jobIdRef.current = jobId;

    try {
      const srcConnId = await ensureConnected(sourceId);
      const tgtConnId = await ensureConnected(targetId);
      if (!srcConnId || !tgtConnId) {
        setSyncState('idle');
        return;
      }

      const mappings = (await syncCommands.compareDataSync(
        srcConnId,
        tgtConnId,
        [],
        jobId,
        sourceDatabase,
        targetDatabase,
      )) as DataSyncTableResult[];
      setMappingResults(mappings);
      const autoSelect = new Set(
        mappings.filter((r) => r.status === 'MATCHED').map((r) => displayTableName(r)),
      );
      setSelectedTables(autoSelect);
      setSyncState('compared');
    } catch (e) {
      setErrorMsg(e instanceof Error ? e.message : String(e));
      setErrorOpen(true);
      setSyncState('idle');
    }
  }, [sourceId, targetId, ensureConnected, t, sourceDatabase, targetDatabase]);

  const handleCancel = useCallback(async () => {
    const jobId = jobIdRef.current;
    if (!jobId) return;
    await syncCommands.cancelDataSync(jobId);
  }, []);

  const handleApply = useCallback(async () => {
    if (!sourceId || !targetId) return;
    const selected = mappingResults.filter(
      (r) =>
        r.status === 'MATCHED' && selectedTables.has(displayTableName(r)) && tableHasRowDiffs(r),
    );
    if (selected.length === 0) return;
    setSyncState('syncing');
    const jobId = crypto.randomUUID();
    jobIdRef.current = jobId;
    try {
      const srcConnId = await ensureConnected(sourceId);
      const tgtConnId = await ensureConnected(targetId);
      if (!srcConnId || !tgtConnId) {
        setSyncState('compared');
        return;
      }
      const result = await syncCommands.applyDataSync(
        srcConnId,
        tgtConnId,
        selected.map((r) => r.sourceTable),
        jobId,
        sourceDatabase,
        targetDatabase,
      );
      if (result.rolledBack) {
        setErrorMsg(t('sync.failedMsg') + t('common.cancel'));
        setErrorOpen(true);
        setSyncState('compared');
        return;
      }
      const mappings = (await syncCommands.compareDataSync(
        srcConnId,
        tgtConnId,
        selected.map((r) => r.sourceTable),
        jobId,
        sourceDatabase,
        targetDatabase,
      )) as DataSyncTableResult[];
      setMappingResults((prev) => {
        const byName = new Map(mappings.map((m) => [m.sourceTable || m.targetTable, m]));
        return prev.map((row) => byName.get(row.sourceTable || row.targetTable) ?? row);
      });
      setSyncState('done');
    } catch (e) {
      setErrorMsg(e instanceof Error ? e.message : String(e));
      setErrorOpen(true);
      setSyncState('compared');
    }
  }, [
    sourceId,
    targetId,
    mappingResults,
    selectedTables,
    ensureConnected,
    t,
    sourceDatabase,
    targetDatabase,
  ]);

  const selectAll = useCallback(() => {
    setSelectedTables(
      new Set(mappingResults.filter((r) => r.status === 'MATCHED').map((r) => displayTableName(r))),
    );
  }, [mappingResults]);

  const deselectAll = useCallback(() => {
    setSelectedTables(new Set());
  }, []);

  const compared = syncState === 'compared' || syncState === 'syncing' || syncState === 'done';

  return (
    <div className="flex h-screen min-h-0 flex-col bg-surface text-fg">
      <TitleBar title={t('sync.windowTitle')} />

      <div
        data-testid="data-sync-overwrite-retired"
        className="shrink-0 border-b border-amber-500/40 bg-amber-500/10 px-6 py-2 text-xs text-fg-secondary"
      >
        {t('sync.overwriteRetiredBanner')}
      </div>

      <div className="flex shrink-0 items-center gap-4 border-b border-edge px-6 py-4">
        <div className="min-w-0 flex-1" data-testid="data-sync-source">
          <label className="mb-1 block text-[11px] font-semibold uppercase tracking-wider text-fg-muted">
            {t('sync.source')}
          </label>
          <Select
            value={sourceId}
            options={connOptions}
            onChange={setSourceId}
            placeholder={t('sync.selectSource')}
          />
          <div data-testid="data-sync-source-database" className="mt-2">
            <Select
              value={sourceDatabase}
              options={sourceDatabases.map((db) => ({ value: db, label: db }))}
              onChange={setSourceDatabase}
              placeholder={t('sync.selectDatabase')}
            />
          </div>
        </div>
        <ArrowRight className="mt-5 h-5 w-5 shrink-0 text-fg-muted" />
        <div className="min-w-0 flex-1" data-testid="data-sync-target">
          <label className="mb-1 block text-[11px] font-semibold uppercase tracking-wider text-fg-muted">
            {t('sync.target')}
          </label>
          <Select
            value={targetId}
            options={targetOptions}
            onChange={setTargetId}
            placeholder={t('sync.selectTarget')}
          />
          <div data-testid="data-sync-target-database" className="mt-2">
            <Select
              value={targetDatabase || ''}
              options={targetDatabases.map((db) => ({ value: db, label: db }))}
              onChange={setTargetDatabase}
              placeholder={t('sync.selectDatabase')}
            />
          </div>
        </div>
        <div className="mt-5 flex shrink-0 flex-col items-end gap-1">
          {activePairing?.supported && (
            <span
              data-testid="data-sync-path"
              className="text-[10px] font-medium uppercase tracking-wide text-fg-muted"
            >
              {activePairing.path === 'direct' ? t('sync.pathDirect') : t('sync.pathIr')}
            </span>
          )}
          <Button
            variant="primary"
            data-testid="data-sync-compare"
            onClick={() => void handleCompare()}
            disabled={syncState === 'comparing' || syncState === 'syncing'}
          >
            {syncState === 'comparing' ? (
              <Loader2 className="h-4 w-4 animate-spin" />
            ) : (
              <RefreshCcw className="h-4 w-4" />
            )}
            {t('sync.compare')}
          </Button>
        </div>
      </div>

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

        {compared && (
          <div className="min-h-0 flex-1 overflow-auto p-4">
            <div className="mb-3 flex items-center gap-2">
              <Button
                variant="ghost"
                className="text-xs"
                data-testid="data-sync-select-all"
                onClick={selectAll}
              >
                {t('common.selectAll')}
              </Button>
              <Button
                variant="ghost"
                className="text-xs"
                data-testid="data-sync-deselect-all"
                onClick={deselectAll}
              >
                {t('common.deselectAll')}
              </Button>
              <div className="flex-1" />
              <span data-testid="data-sync-mapping-summary" className="text-xs text-fg-muted">
                {t('sync.mappingSummary', summarizeMappings(mappingResults))}
              </span>
            </div>

            <div className="flex items-center gap-3 rounded-t-lg border border-edge bg-surface-alt px-3 py-2 text-[11px] font-semibold uppercase tracking-wider text-fg-muted">
              <div className="w-6" />
              <div className="min-w-0 flex-1">{t('sync.tableName')}</div>
              <div className="w-40 text-right">{t('sync.status')}</div>
            </div>

            {mappingResults.map((row) => {
              const name = displayTableName(row);
              const isSelected = selectedTables.has(name);
              const disabled = row.status !== 'MATCHED';
              return (
                <div
                  key={`${row.status}:${name}`}
                  data-testid="data-sync-mapping-row"
                  className={cn(
                    'flex items-center gap-3 border-b border-edge px-3 py-2 text-sm',
                    disabled && 'opacity-60',
                  )}
                >
                  <input
                    type="checkbox"
                    className="h-3.5 w-3.5"
                    checked={isSelected}
                    disabled={disabled}
                    onChange={() => {
                      if (disabled) return;
                      setSelectedTables((prev) => {
                        const next = new Set(prev);
                        if (next.has(name)) next.delete(name);
                        else next.add(name);
                        return next;
                      });
                    }}
                  />
                  <div className="min-w-0 flex-1 truncate font-mono text-xs">{name}</div>
                  <div className="w-56 text-right text-xs text-fg-secondary">
                    {row.status === 'MATCHED' && tableHasRowDiffs(row)
                      ? t('sync.rowDiffs', rowDiffCounts(row))
                      : t(mappingLabelKey(row.status))}
                  </div>
                  {row.incompatibleReason && (
                    <div
                      className="max-w-xs truncate text-[11px] text-amber-600 dark:text-amber-400"
                      title={row.incompatibleReason}
                    >
                      {row.incompatibleReason}
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        )}
      </div>

      {compared && (
        <div className="flex shrink-0 items-center gap-3 border-t border-edge px-6 py-3">
          <span data-testid="data-sync-selected" className="text-xs text-fg-muted">
            {t('sync.selected', { selected: selectedTables.size, total: mappingResults.length })}
          </span>
          <div className="flex-1" />
          {(syncState === 'comparing' || syncState === 'syncing') && (
            <Button
              variant="ghost"
              data-testid="data-sync-cancel"
              onClick={() => void handleCancel()}
            >
              {t('common.cancel')}
            </Button>
          )}
          <Button
            variant="primary"
            onClick={() => void handleApply()}
            disabled={
              syncState === 'comparing' ||
              syncState === 'syncing' ||
              !mappingResults.some(
                (r) =>
                  r.status === 'MATCHED' &&
                  selectedTables.has(displayTableName(r)) &&
                  tableHasRowDiffs(r),
              )
            }
            title={
              mappingResults.some(
                (r) =>
                  r.status === 'MATCHED' &&
                  selectedTables.has(displayTableName(r)) &&
                  tableHasRowDiffs(r),
              )
                ? undefined
                : t('sync.applyUnavailable')
            }
            data-testid={
              mappingResults.some(
                (r) =>
                  r.status === 'MATCHED' &&
                  selectedTables.has(displayTableName(r)) &&
                  tableHasRowDiffs(r),
              )
                ? 'data-sync-start'
                : 'data-sync-start-disabled'
            }
          >
            {syncState === 'syncing' ? (
              <Loader2 className="h-4 w-4 animate-spin" />
            ) : (
              <RefreshCcw className="h-4 w-4" />
            )}
            {t('sync.startSync')}
          </Button>
        </div>
      )}

      <StatusBar
        left={<span className="truncate">{t('sync.title')}</span>}
        right={<span className="tabular-nums">DataZen v0.0.9</span>}
      />

      <Dialog
        open={errorOpen}
        title={t('common.hint')}
        onClose={() => setErrorOpen(false)}
        footer={
          <Button variant="primary" onClick={() => setErrorOpen(false)}>
            {t('common.ok')}
          </Button>
        }
      >
        <p
          data-testid="data-sync-error"
          className="whitespace-pre-wrap break-all text-sm text-fg-secondary"
        >
          {errorMsg}
        </p>
      </Dialog>
    </div>
  );
}
