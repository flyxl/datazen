import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Loader2 } from 'lucide-react';
import { TitleBar } from '../../components/TitleBar';
import { StatusBar } from '../../components/StatusBar';
import { Button } from '../../components/ui/Button';
import { Dialog } from '../../components/ui/Dialog';
import { aiCommands } from '../../commands/ai';
import {
  DEFAULT_SYNC_OPTIONS,
  syncCommands,
  type DataSyncRowChange,
  type SyncOptions,
} from '../../commands/sync';
import { databaseCommands } from '../../commands/database';
import { useI18n } from '../../hooks/useI18n';
import { useSettings } from '../../hooks/useSettings';
import { useSettingsStore } from '../../stores/settingsStore';
import { useAiStore } from '../../stores/aiStore';
import { listenCrossWindow } from '../../lib/crossWindowBus';
import { openDataTransferWindow, openSchemaDiffWindow } from '../../lib/windowManager';
import { resolveSyncPairing, isSyncTargetSupported } from '../../lib/syncPairing';
import type { ConnectionConfig } from '../../types';
import type { SyncState } from './utils';
import { pickDefaultSchema, uniqueSchemasFromTables } from './utils';
import { CompareSummary } from './CompareSummary';
import { DiffDetail } from './DiffDetail';
import { EndpointsBar } from './EndpointsBar';
import { ExecuteBar } from './ExecuteBar';
import { MappingPanel } from './MappingPanel';
import { OptionsBar } from './OptionsBar';
import { SqlPreview } from './SqlPreview';
import { TableListPanel } from './TableListPanel';
import {
  applyOptionsToRows,
  markDisabledTables,
  mergeCompareIntoMappings,
  operationAllowed,
  selectedRowCount,
  summarizeCompare,
  tableHasRowDiffs,
  tableKey,
  tablesForCompare,
  type DataSyncTableResult,
  type TableDiffFilter,
} from './mappingView';
import { buildCompareReportText } from './compareReport';

type RightPanel = 'detail' | 'preview';

export function DataSyncWindow() {
  useSettings();
  const { t } = useI18n();
  const loadSettings = useSettingsStore((s) => s.loadSettings);
  const isAiConfigured = useAiStore((s) => s.isConfigured);
  const loadAiConfig = useAiStore((s) => s.loadConfig);

  const [connections, setConnections] = useState<ConnectionConfig[]>([]);
  const [activeConns, setActiveConns] = useState<Record<string, string>>({});
  const [sourceId, setSourceId] = useState('');
  const [targetId, setTargetId] = useState('');
  const [sourceDatabases, setSourceDatabases] = useState<string[]>([]);
  const [targetDatabases, setTargetDatabases] = useState<string[]>([]);
  const [sourceDatabase, setSourceDatabase] = useState('');
  const [targetDatabase, setTargetDatabase] = useState('');
  const [sourceSchemas, setSourceSchemas] = useState<string[]>([]);
  const [targetSchemas, setTargetSchemas] = useState<string[]>([]);
  const [sourceSchema, setSourceSchema] = useState('');
  const [targetSchema, setTargetSchema] = useState('');
  const [syncOptions, setSyncOptions] = useState<SyncOptions>(DEFAULT_SYNC_OPTIONS);
  const [mappingResults, setMappingResults] = useState<DataSyncTableResult[]>([]);
  const [disabledTables, setDisabledTables] = useState<Set<string>>(new Set());
  const [syncState, setSyncState] = useState<SyncState>('idle');
  const [selectedTableKey, setSelectedTableKey] = useState<string | null>(null);
  const [tableFilter, setTableFilter] = useState<TableDiffFilter>('all');
  const [tableSearch, setTableSearch] = useState('');
  const [rightPanel, setRightPanel] = useState<RightPanel>('detail');
  const [errorMsg, setErrorMsg] = useState('');
  const [errorOpen, setErrorOpen] = useState(false);
  const [deleteConfirmOpen, setDeleteConfirmOpen] = useState(false);
  const [executeConfirmOpen, setExecuteConfirmOpen] = useState(false);
  const [explainOpen, setExplainOpen] = useState(false);
  const [explainLoading, setExplainLoading] = useState(false);
  const [explainText, setExplainText] = useState('');
  const jobIdRef = useRef<string | null>(null);

  useEffect(() => {
    void loadSettings();
    void loadAiConfig();
  }, [loadSettings, loadAiConfig]);

  useEffect(() => {
    let cleanup: (() => void) | undefined;
    listenCrossWindow('datazen:connection-closed', (payload) => {
      const { dbSessionId } = (payload ?? {}) as { dbSessionId?: string };
      if (!dbSessionId) return;
      setActiveConns((prev) => {
        const next = { ...prev };
        for (const [connectionId, sessionId] of Object.entries(next)) {
          if (sessionId === dbSessionId) delete next[connectionId];
        }
        return next;
      });
    }).then((fn) => {
      cleanup = fn;
    });
    return () => cleanup?.();
  }, []);

  const loadConnections = useCallback(() => {
    void invoke<ConnectionConfig[]>('get_connections')
      .then(setConnections)
      .catch((e) => console.error('Failed to load', e));
  }, []);

  useEffect(() => {
    loadConnections();
  }, [loadConnections]);

  useEffect(() => {
    let cleanup: (() => void) | undefined;
    listenCrossWindow('datazen:connections-changed', loadConnections).then((fn) => {
      cleanup = fn;
    });
    return () => cleanup?.();
  }, [loadConnections]);

  const sourceConn = useMemo(
    () => connections.find((c) => c.id === sourceId),
    [connections, sourceId],
  );
  const targetConn = useMemo(
    () => connections.find((c) => c.id === targetId),
    [connections, targetId],
  );

  const connOptions = useMemo(
    () =>
      connections.map((c) => ({
        value: c.id,
        label: `${c.name} (${c.databaseType})`,
      })),
    [connections],
  );

  const targetOptions = useMemo(() => {
    const hint = t('common.unsupportedPair');
    const srcType = sourceConn?.databaseType;
    return connections.map((c) => {
      const unsupported = Boolean(srcType && !isSyncTargetSupported(srcType, c.databaseType));
      const base = `${c.name} (${c.databaseType})`;
      return {
        value: c.id,
        label: unsupported ? `${base} — ${hint}` : base,
        disabled: unsupported,
        title: unsupported ? hint : undefined,
      };
    });
  }, [connections, sourceConn?.databaseType, t]);

  const activePairing = useMemo(() => {
    if (!sourceConn || !targetId) return null;
    const tgt = connections.find((c) => c.id === targetId);
    if (!tgt) return null;
    return resolveSyncPairing(sourceConn.databaseType, tgt.databaseType);
  }, [connections, sourceConn, targetId]);

  const targetReadOnly = targetConn?.readOnly === true;

  const ensureConnected = useCallback(
    async (connectionId: string): Promise<string | null> => {
      // activeConns maps persistent connection id -> live db session id.
      if (activeConns[connectionId]) return activeConns[connectionId];
      try {
        const dbSessionId = await invoke<string>('connect', { connectionId });
        setActiveConns((prev) => ({ ...prev, [connectionId]: dbSessionId }));
        return dbSessionId;
      } catch (e) {
        setErrorMsg(`${t('sync.connectFailed')} ${e instanceof Error ? e.message : String(e)}`);
        setErrorOpen(true);
        return null;
      }
    },
    [activeConns, t],
  );

  const resetCompareState = useCallback(() => {
    setMappingResults([]);
    setDisabledTables(new Set());
    setSelectedTableKey(null);
    setSyncState('idle');
  }, []);

  useEffect(() => {
    if (!sourceId) {
      setSourceDatabases([]);
      setSourceSchemas([]);
      setSourceSchema('');
      return;
    }
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

  useEffect(() => {
    if (!targetId) {
      setTargetDatabases([]);
      setTargetSchemas([]);
      setTargetSchema('');
      return;
    }
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

  useEffect(() => {
    if (!sourceId || !sourceDatabase || sourceConn?.databaseType !== 'postgresql') {
      setSourceSchemas([]);
      setSourceSchema('');
      return;
    }
    let cancelled = false;
    (async () => {
      try {
        const connId = await ensureConnected(sourceId);
        if (!connId || cancelled) return;
        const tables = await databaseCommands.getTables(connId, sourceDatabase);
        if (cancelled) return;
        const schemas = uniqueSchemasFromTables(tables);
        setSourceSchemas(schemas);
        setSourceSchema((prev) => pickDefaultSchema(schemas, prev));
      } catch {
        if (!cancelled) {
          setSourceSchemas([]);
          setSourceSchema('');
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [sourceId, sourceDatabase, sourceConn?.databaseType, ensureConnected]);

  useEffect(() => {
    if (!targetId || !targetDatabase || targetConn?.databaseType !== 'postgresql') {
      setTargetSchemas([]);
      setTargetSchema('');
      return;
    }
    let cancelled = false;
    (async () => {
      try {
        const connId = await ensureConnected(targetId);
        if (!connId || cancelled) return;
        const tables = await databaseCommands.getTables(connId, targetDatabase);
        if (cancelled) return;
        const schemas = uniqueSchemasFromTables(tables);
        setTargetSchemas(schemas);
        setTargetSchema((prev) => pickDefaultSchema(schemas, prev));
      } catch {
        if (!cancelled) {
          setTargetSchemas([]);
          setTargetSchema('');
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [targetId, targetDatabase, targetConn?.databaseType, ensureConnected]);

  const isSameEndpoint = useCallback(() => {
    const norm = (s: string) => s.trim();
    return (
      sourceId === targetId &&
      sourceDatabase !== '' &&
      targetDatabase !== '' &&
      sourceDatabase === targetDatabase &&
      norm(sourceSchema) === norm(targetSchema)
    );
  }, [sourceId, targetId, sourceDatabase, targetDatabase, sourceSchema, targetSchema]);

  const handleSwap = useCallback(() => {
    setSourceId(targetId);
    setTargetId(sourceId);
    setSourceDatabase(targetDatabase);
    setTargetDatabase(sourceDatabase);
    setSourceDatabases(targetDatabases);
    setTargetDatabases(sourceDatabases);
    setSourceSchemas(targetSchemas);
    setTargetSchemas(sourceSchemas);
    setSourceSchema(targetSchema);
    setTargetSchema(sourceSchema);
    resetCompareState();
  }, [
    sourceId,
    targetId,
    sourceDatabase,
    targetDatabase,
    sourceDatabases,
    targetDatabases,
    sourceSchemas,
    targetSchemas,
    sourceSchema,
    targetSchema,
    resetCompareState,
  ]);

  const handleSourceDatabaseChange = useCallback(
    (db: string) => {
      setSourceDatabase(db);
      resetCompareState();
    },
    [resetCompareState],
  );

  const handleTargetDatabaseChange = useCallback(
    (db: string) => {
      setTargetDatabase(db);
      resetCompareState();
    },
    [resetCompareState],
  );

  const handleSourceSchemaChange = useCallback(
    (schema: string) => {
      setSourceSchema(schema);
      resetCompareState();
    },
    [resetCompareState],
  );

  const handleTargetSchemaChange = useCallback(
    (schema: string) => {
      setTargetSchema(schema);
      resetCompareState();
    },
    [resetCompareState],
  );

  const validateEndpoints = useCallback((): boolean => {
    if (!sourceId || !targetId) {
      setErrorMsg(t('sync.selectBoth'));
      setErrorOpen(true);
      return false;
    }
    if (isSameEndpoint()) {
      setErrorMsg(t('sync.cannotSameDb'));
      setErrorOpen(true);
      return false;
    }
    if (!sourceDatabase || !targetDatabase) {
      setErrorMsg(t('sync.selectDbRequired'));
      setErrorOpen(true);
      return false;
    }
    if (!activePairing?.supported) {
      setErrorMsg(t('sync.useTransferHint'));
      setErrorOpen(true);
      return false;
    }
    return true;
  }, [sourceId, targetId, isSameEndpoint, sourceDatabase, targetDatabase, activePairing, t]);

  const handleCompare = useCallback(async () => {
    if (!validateEndpoints()) return;

    setSyncState('inspecting');
    setSelectedTableKey(null);
    const jobId = crypto.randomUUID();
    jobIdRef.current = jobId;

    try {
      const srcConnId = await ensureConnected(sourceId);
      const tgtConnId = await ensureConnected(targetId);
      if (!srcConnId || !tgtConnId) {
        setSyncState('idle');
        return;
      }

      const inspected = await syncCommands.inspectDataSync(
        srcConnId,
        tgtConnId,
        sourceDatabase,
        targetDatabase,
        sourceSchema || undefined,
        targetSchema || undefined,
      );
      const withDisabled = markDisabledTables(inspected, disabledTables);
      setMappingResults(withDisabled);

      const toCompare = tablesForCompare(withDisabled);
      setSyncState('comparing');

      const compared = await syncCommands.compareDataSync(
        srcConnId,
        tgtConnId,
        toCompare,
        jobId,
        sourceDatabase,
        targetDatabase,
        sourceSchema || undefined,
        targetSchema || undefined,
        syncOptions,
      );

      const merged = mergeCompareIntoMappings(withDisabled, compared).map((row) => {
        if (row.status !== 'MATCHED' || !row.rows) return row;
        return {
          ...row,
          rows: applyOptionsToRows(row.rows, syncOptions),
        };
      });
      setMappingResults(merged);
      const firstDiff = merged.find((r) => r.status === 'MATCHED' && tableHasRowDiffs(r));
      if (firstDiff) setSelectedTableKey(tableKey(firstDiff));
      setSyncState('compared');
    } catch (e) {
      setErrorMsg(e instanceof Error ? e.message : String(e));
      setErrorOpen(true);
      setSyncState('idle');
    }
  }, [
    validateEndpoints,
    ensureConnected,
    sourceId,
    targetId,
    sourceDatabase,
    targetDatabase,
    sourceSchema,
    targetSchema,
    disabledTables,
    syncOptions,
  ]);

  const handleCancel = useCallback(async () => {
    const jobId = jobIdRef.current;
    if (!jobId) return;
    await syncCommands.cancelDataSync(jobId);
  }, []);

  const toggleDisabledTable = useCallback((sourceTable: string) => {
    setDisabledTables((prev) => {
      const next = new Set(prev);
      if (next.has(sourceTable)) next.delete(sourceTable);
      else next.add(sourceTable);
      return next;
    });
    setMappingResults((rows) =>
      rows.map((r) => {
        if (r.sourceTable !== sourceTable) return r;
        if (r.status === 'DISABLED') return { ...r, status: 'MATCHED' as const };
        if (r.status === 'MATCHED') return { ...r, status: 'DISABLED' as const, rows: undefined };
        return r;
      }),
    );
  }, []);

  const handleOptionsChange = useCallback((next: SyncOptions) => {
    setSyncOptions(next);
    setMappingResults((rows) =>
      rows.map((row) => {
        if (!row.rows) return row;
        return { ...row, rows: applyOptionsToRows(row.rows, next) };
      }),
    );
  }, []);

  const handleEnableDelete = useCallback(() => {
    setDeleteConfirmOpen(true);
  }, []);

  const confirmEnableDelete = useCallback(() => {
    setSyncOptions((prev) => ({ ...prev, delete: true }));
    setDeleteConfirmOpen(false);
  }, []);

  const selectedTable = useMemo(
    () => mappingResults.find((r) => tableKey(r) === selectedTableKey) ?? null,
    [mappingResults, selectedTableKey],
  );

  const totalSelectedRows = useMemo(() => {
    let n = 0;
    for (const row of mappingResults) {
      n += selectedRowCount(row, syncOptions);
    }
    return n;
  }, [mappingResults, syncOptions]);

  const hasSelectedDeletes = useMemo(() => {
    for (const table of mappingResults) {
      for (const row of table.rows ?? []) {
        if (row.selected && row.operation === 'DELETE' && syncOptions.delete) return true;
      }
    }
    return false;
  }, [mappingResults, syncOptions]);

  const runExecute = useCallback(async () => {
    if (!sourceId || !targetId) return;
    setSyncState('executing');
    const jobId = crypto.randomUUID();
    jobIdRef.current = jobId;

    try {
      const srcConnId = await ensureConnected(sourceId);
      const tgtConnId = await ensureConnected(targetId);
      if (!srcConnId || !tgtConnId) {
        setSyncState('compared');
        return;
      }

      const tablesWithSelection = mappingResults.filter(
        (r) => r.status === 'MATCHED' && selectedRowCount(r, syncOptions) > 0,
      );

      let executed = false;
      try {
        const stmts = await syncCommands.generateDataSyncSql(
          srcConnId,
          tgtConnId,
          tablesWithSelection,
          syncOptions,
          sourceDatabase,
          targetDatabase,
          sourceSchema || undefined,
          targetSchema || undefined,
        );
        const selected = stmts.filter((s) => operationAllowed(s.operation, syncOptions));
        if (selected.length > 0) {
          const result = await syncCommands.executeDataSync(
            tgtConnId,
            selected,
            jobId,
            targetDatabase,
          );
          if (result.rolledBack) {
            setErrorMsg(t('sync.failedMsg') + t('common.cancel'));
            setErrorOpen(true);
            setSyncState('compared');
            return;
          }
          executed = true;
        }
      } catch {
        /* backend generate not available */
      }

      if (!executed) {
        const tableNames = tablesWithSelection.map((r) => r.sourceTable);
        const result = await syncCommands.applyDataSync(
          srcConnId,
          tgtConnId,
          tableNames,
          jobId,
          sourceDatabase,
          targetDatabase,
          sourceSchema || undefined,
          targetSchema || undefined,
          syncOptions,
        );
        if (result.rolledBack) {
          setErrorMsg(t('sync.failedMsg') + t('common.cancel'));
          setErrorOpen(true);
          setSyncState('compared');
          return;
        }
      }

      const recompared = await syncCommands.compareDataSync(
        srcConnId,
        tgtConnId,
        tablesWithSelection.map((r) => r.sourceTable),
        jobId,
        sourceDatabase,
        targetDatabase,
        sourceSchema || undefined,
        targetSchema || undefined,
        syncOptions,
      );
      setMappingResults((prev) => {
        const merged = mergeCompareIntoMappings(prev, recompared);
        return merged.map((row) => {
          if (row.status !== 'MATCHED' || !row.rows) return row;
          return { ...row, rows: applyOptionsToRows(row.rows, syncOptions) };
        });
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
    syncOptions,
    ensureConnected,
    sourceDatabase,
    targetDatabase,
    sourceSchema,
    targetSchema,
    t,
  ]);

  const handleExecute = useCallback(() => {
    if (targetReadOnly) return;
    if (hasSelectedDeletes) {
      setExecuteConfirmOpen(true);
      return;
    }
    void runExecute();
  }, [targetReadOnly, hasSelectedDeletes, runExecute]);

  const updateTableRows = useCallback((key: string, rows: DataSyncRowChange[]) => {
    setMappingResults((prev) => prev.map((r) => (tableKey(r) === key ? { ...r, rows } : r)));
  }, []);

  const compared = syncState === 'compared' || syncState === 'executing' || syncState === 'done';
  const busy = syncState === 'inspecting' || syncState === 'comparing' || syncState === 'executing';
  const compareStats = useMemo(() => summarizeCompare(mappingResults), [mappingResults]);

  const handleCopyCompareReport = useCallback(() => {
    const text = buildCompareReportText(mappingResults, compareStats);
    void navigator.clipboard.writeText(text);
  }, [mappingResults, compareStats]);

  const handleExplainDiff = useCallback(() => {
    if (!isAiConfigured) {
      setErrorMsg(t('sync.explainDiffNoAi'));
      setErrorOpen(true);
      return;
    }
    const report = buildCompareReportText(mappingResults, compareStats);
    const prompt = t('sync.explainDiffPrompt', { report });
    setExplainOpen(true);
    setExplainLoading(true);
    setExplainText('');
    void (async () => {
      try {
        const connectionId = activeConns[sourceId] ?? activeConns[targetId];
        const text = await aiCommands.chat({
          dbSessionId: connectionId,
          database: sourceDatabase || targetDatabase || undefined,
          messages: [{ role: 'user', content: prompt }],
          requestId: crypto.randomUUID(),
          includeSchema: false,
        });
        setExplainText(text);
      } catch (e) {
        setExplainText('');
        setExplainOpen(false);
        setErrorMsg(
          `${t('sync.explainDiffFailed')}: ${e instanceof Error ? e.message : String(e)}`,
        );
        setErrorOpen(true);
      } finally {
        setExplainLoading(false);
      }
    })();
  }, [
    isAiConfigured,
    mappingResults,
    compareStats,
    activeConns,
    sourceId,
    targetId,
    sourceDatabase,
    targetDatabase,
    t,
  ]);

  return (
    <div className="flex h-screen min-h-0 flex-col bg-surface text-fg">
      <TitleBar title={t('common.dataSyncTitle')} />

      <div
        data-testid="data-sync-overwrite-retired"
        className="shrink-0 border-b border-amber-500/40 bg-amber-500/10 px-6 py-2 text-xs text-fg-secondary"
      >
        {t('sync.overwriteRetiredBanner')}
      </div>

      <EndpointsBar
        sourceId={sourceId}
        targetId={targetId}
        sourceDatabase={sourceDatabase}
        targetDatabase={targetDatabase}
        sourceSchema={sourceSchema}
        targetSchema={targetSchema}
        sourceDatabases={sourceDatabases}
        targetDatabases={targetDatabases}
        sourceSchemas={sourceSchemas}
        targetSchemas={targetSchemas}
        connOptions={connOptions}
        targetOptions={targetOptions}
        activePairing={activePairing}
        busy={busy}
        onSourceChange={setSourceId}
        onTargetChange={setTargetId}
        onSourceDatabaseChange={handleSourceDatabaseChange}
        onTargetDatabaseChange={handleTargetDatabaseChange}
        onSourceSchemaChange={handleSourceSchemaChange}
        onTargetSchemaChange={handleTargetSchemaChange}
        onSwap={handleSwap}
        onCompare={() => void handleCompare()}
      />

      <OptionsBar
        options={syncOptions}
        onChange={handleOptionsChange}
        onEnableDelete={handleEnableDelete}
      />

      <div className="min-h-0 flex-1 overflow-hidden flex flex-col">
        {syncState === 'idle' && mappingResults.length === 0 && (
          <div className="flex flex-1 items-center justify-center text-sm text-fg-muted">
            {t('sync.selectPrompt')}
          </div>
        )}

        {(syncState === 'inspecting' || syncState === 'comparing') && (
          <div className="flex flex-1 items-center justify-center gap-2 text-sm text-fg-muted">
            <Loader2 className="h-4 w-4 animate-spin" />
            {syncState === 'inspecting' ? t('sync.inspecting') : t('sync.comparing')}
            <Button
              variant="ghost"
              size="sm"
              data-testid="data-sync-cancel"
              onClick={() => void handleCancel()}
            >
              {t('common.cancel')}
            </Button>
          </div>
        )}

        {mappingResults.length > 0 && syncState !== 'inspecting' && syncState !== 'comparing' && (
          <>
            <MappingPanel
              rows={mappingResults}
              disabledTables={disabledTables}
              compared={compared}
              onToggleDisabled={toggleDisabledTable}
              onOpenSchemaDiff={openSchemaDiffWindow}
              onOpenDataTransfer={openDataTransferWindow}
            />

            {compared && (
              <CompareSummary
                stats={compareStats}
                onCopyReport={handleCopyCompareReport}
                onExplainDiff={handleExplainDiff}
                explainLoading={explainLoading}
              />
            )}

            {compared && (
              <div className="flex min-h-0 flex-1">
                <TableListPanel
                  rows={mappingResults}
                  filter={tableFilter}
                  search={tableSearch}
                  selectedTableKey={selectedTableKey}
                  onFilterChange={setTableFilter}
                  onSearchChange={setTableSearch}
                  onSelectTable={(key) => {
                    setSelectedTableKey(key);
                    setRightPanel('detail');
                  }}
                />

                <div className="flex min-h-0 min-w-0 flex-[1.4] flex-col">
                  <div className="flex shrink-0 border-b border-edge">
                    <button
                      type="button"
                      className={`px-4 py-2 text-xs font-medium ${rightPanel === 'detail' ? 'border-b-2 border-accent text-fg' : 'text-fg-muted'}`}
                      onClick={() => setRightPanel('detail')}
                    >
                      {t('sync.rowDiffTab')}
                    </button>
                    <button
                      type="button"
                      className={`px-4 py-2 text-xs font-medium ${rightPanel === 'preview' ? 'border-b-2 border-accent text-fg' : 'text-fg-muted'}`}
                      onClick={() => setRightPanel('preview')}
                    >
                      {t('common.sqlPreviewLower')}
                    </button>
                  </div>

                  <div className="min-h-0 flex-1 overflow-hidden flex flex-col">
                    {rightPanel === 'detail' && selectedTable && (
                      <DiffDetail
                        table={selectedTable}
                        options={syncOptions}
                        onUpdateRows={(rows) => updateTableRows(tableKey(selectedTable), rows)}
                      />
                    )}
                    {rightPanel === 'detail' && !selectedTable && (
                      <div className="flex flex-1 items-center justify-center text-sm text-fg-muted">
                        {t('sync.selectTableForDetail')}
                      </div>
                    )}
                    {rightPanel === 'preview' && activeConns[sourceId] && activeConns[targetId] && (
                      <SqlPreview
                        sourceConnId={activeConns[sourceId]}
                        targetConnId={activeConns[targetId]}
                        sourceDatabase={sourceDatabase}
                        targetDatabase={targetDatabase}
                        sourceSchema={sourceSchema}
                        targetSchema={targetSchema}
                        tables={mappingResults}
                        options={syncOptions}
                      />
                    )}
                  </div>
                </div>
              </div>
            )}
          </>
        )}
      </div>

      {compared && (
        <ExecuteBar
          selectedRows={totalSelectedRows}
          hasDeletes={hasSelectedDeletes}
          targetReadOnly={targetReadOnly}
          executing={syncState === 'executing'}
          canExecute={mappingResults.some((r) => r.status === 'MATCHED' && tableHasRowDiffs(r))}
          onExecute={() => void handleExecute()}
          onCancel={() => void handleCancel()}
        />
      )}

      <StatusBar
        left={<span className="truncate">{t('common.dataSync')}</span>}
        right={<span className="tabular-nums">DataZen v0.1.0</span>}
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

      <Dialog
        open={deleteConfirmOpen}
        title={t('sync.deleteConfirmTitle')}
        onClose={() => setDeleteConfirmOpen(false)}
        footer={
          <>
            <Button variant="ghost" onClick={() => setDeleteConfirmOpen(false)}>
              {t('common.cancel')}
            </Button>
            <Button variant="danger" onClick={confirmEnableDelete}>
              {t('sync.enableDelete')}
            </Button>
          </>
        }
      >
        <p className="text-sm text-fg-secondary">{t('sync.deleteConfirmBody')}</p>
      </Dialog>

      <Dialog
        open={explainOpen}
        title={t('sync.explainDiffTitle')}
        onClose={() => setExplainOpen(false)}
        footer={
          <Button variant="primary" onClick={() => setExplainOpen(false)}>
            {t('common.close')}
          </Button>
        }
      >
        {explainLoading ? (
          <div className="flex items-center gap-2 text-sm text-fg-muted">
            <Loader2 className="h-4 w-4 animate-spin" />
            {t('query.executing')}
          </div>
        ) : (
          <p
            data-testid="data-sync-explain-result"
            className="whitespace-pre-wrap text-sm text-fg-secondary"
          >
            {explainText}
          </p>
        )}
      </Dialog>

      <Dialog
        open={executeConfirmOpen}
        title={t('sync.executeDeleteTitle')}
        onClose={() => setExecuteConfirmOpen(false)}
        footer={
          <>
            <Button variant="ghost" onClick={() => setExecuteConfirmOpen(false)}>
              {t('common.cancel')}
            </Button>
            <Button
              variant="danger"
              onClick={() => {
                setExecuteConfirmOpen(false);
                void runExecute();
              }}
            >
              {t('sync.execute')}
            </Button>
          </>
        }
      >
        <p className="text-sm text-fg-secondary">{t('sync.executeDeleteBody')}</p>
      </Dialog>
    </div>
  );
}
