import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { ChevronLeft, ChevronRight, Loader2 } from 'lucide-react';
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
import { cn } from '../../lib/cn';
import { openDataTransferWindow, openSchemaDiffWindow } from '../../lib/windowManager';
import {
  ensureDedicatedSession,
  listDatabasesDedicated,
  releaseDedicatedSession,
  type DedicatedSideSession,
} from '../../lib/dedicatedDbSession';
import { useSyncPairingState } from '../../lib/syncPairing';
import { DB_REGISTRY } from '../../lib/databaseTypes';
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

type WizardStep = 'endpoints' | 'setup' | 'objects' | 'compare' | 'preview' | 'result';

const STEPS: WizardStep[] = ['endpoints', 'setup', 'objects', 'compare', 'preview', 'result'];

const NARROW_STEPS: WizardStep[] = ['endpoints', 'setup', 'result'];

export function DataSyncWindow() {
  useSettings();
  const { t } = useI18n();
  const loadSettings = useSettingsStore((s) => s.loadSettings);
  const isAiConfigured = useAiStore((s) => s.isConfigured);
  const loadAiConfig = useAiStore((s) => s.loadConfig);

  const [connections, setConnections] = useState<ConnectionConfig[]>([]);
  const [sourceSession, setSourceSession] = useState<DedicatedSideSession | null>(null);
  const [targetSession, setTargetSession] = useState<DedicatedSideSession | null>(null);
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
  const [step, setStep] = useState<WizardStep>('endpoints');
  const [inspectionComplete, setInspectionComplete] = useState(false);
  const [syncState, setSyncState] = useState<SyncState>('idle');
  const [selectedTableKey, setSelectedTableKey] = useState<string | null>(null);
  const [tableFilter, setTableFilter] = useState<TableDiffFilter>('all');
  const [tableSearch, setTableSearch] = useState('');
  const [errorMsg, setErrorMsg] = useState('');
  const [errorOpen, setErrorOpen] = useState(false);
  const [statusMsg, setStatusMsg] = useState('');
  const [sourceSessionError, setSourceSessionError] = useState('');
  const [targetSessionError, setTargetSessionError] = useState('');
  const [deleteConfirmOpen, setDeleteConfirmOpen] = useState(false);
  const [executeConfirmOpen, setExecuteConfirmOpen] = useState(false);
  const [explainOpen, setExplainOpen] = useState(false);
  const [explainLoading, setExplainLoading] = useState(false);
  const [explainText, setExplainText] = useState('');
  const jobIdRef = useRef<string | null>(null);
  const compareGenerationRef = useRef(0);

  useEffect(() => {
    void loadSettings();
    void loadAiConfig();
  }, [loadSettings, loadAiConfig]);

  useEffect(() => {
    let cleanup: (() => void) | undefined;
    listenCrossWindow('datazen:connection-closed', (payload) => {
      const { dbSessionId } = (payload ?? {}) as { dbSessionId?: string };
      if (!dbSessionId) return;
      setSourceSession((prev) => (prev?.dbSessionId === dbSessionId ? null : prev));
      setTargetSession((prev) => (prev?.dbSessionId === dbSessionId ? null : prev));
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

  const { targetSupport, activePairing } = useSyncPairingState(
    sourceConn?.databaseType,
    connections,
    targetId,
  );

  const targetOptions = useMemo(() => {
    const hint = t('common.unsupportedPair');
    const srcType = sourceConn?.databaseType;
    return connections.map((c) => {
      const unsupported = Boolean(
        srcType && Object.hasOwn(targetSupport, c.id) && !targetSupport[c.id],
      );
      const base = `${c.name} (${c.databaseType})`;
      return {
        value: c.id,
        label: unsupported ? `${base} — ${hint}` : base,
        disabled: unsupported,
        title: unsupported ? hint : undefined,
      };
    });
  }, [connections, sourceConn?.databaseType, targetSupport, t]);

  const targetReadOnly = targetConn?.readOnly === true;

  useEffect(() => {
    const dbSessionId = sourceSession?.dbSessionId;
    return () => {
      void releaseDedicatedSession(dbSessionId);
    };
  }, [sourceSession?.dbSessionId]);

  useEffect(() => {
    const dbSessionId = targetSession?.dbSessionId;
    return () => {
      void releaseDedicatedSession(dbSessionId);
    };
  }, [targetSession?.dbSessionId]);

  const refreshEndpointSessions = useCallback(async () => {
    if (!sourceId || !targetId || !sourceDatabase || !targetDatabase) {
      return {
        source: null as DedicatedSideSession | null,
        target: null as DedicatedSideSession | null,
      };
    }
    try {
      const [source, target] = await Promise.all([
        ensureDedicatedSession(sourceSession, sourceId, sourceDatabase),
        ensureDedicatedSession(targetSession, targetId, targetDatabase),
      ]);
      setSourceSession(source);
      setTargetSession(target);
      return { source, target };
    } catch (e) {
      setErrorMsg(`${t('sync.connectFailed')} ${e instanceof Error ? e.message : String(e)}`);
      setErrorOpen(true);
      return { source: null, target: null };
    }
  }, [sourceSession, targetSession, sourceId, targetId, sourceDatabase, targetDatabase, t]);

  const resetCompareState = useCallback(() => {
    setMappingResults([]);
    setDisabledTables(new Set());
    setInspectionComplete(false);
    setSelectedTableKey(null);
    setSyncState('idle');
    setStep('endpoints');
  }, []);

  useEffect(() => {
    if (!sourceId) {
      setSourceDatabases([]);
      setSourceSchemas([]);
      setSourceSchema('');
      setSourceSession(null);
      return;
    }
    let cancelled = false;
    const cfg = connections.find((c) => c.id === sourceId);
    (async () => {
      try {
        const { databases } = await listDatabasesDedicated(sourceId, cfg?.database);
        if (cancelled) return;
        setSourceDatabases(databases);
        const preferred = cfg?.database ?? '';
        setSourceDatabase((prev) =>
          databases.includes(preferred)
            ? preferred
            : prev && databases.includes(prev)
              ? prev
              : (databases[0] ?? ''),
        );
      } catch (e) {
        if (!cancelled) {
          setSourceDatabases([]);
          setErrorMsg(
            `${t('sync.loadDatabasesFailed')}${e instanceof Error ? e.message : String(e)}`,
          );
          setErrorOpen(true);
        }
      }
    })();
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sourceId, connections]);

  useEffect(() => {
    if (!targetId) {
      setTargetDatabases([]);
      setTargetSchemas([]);
      setTargetSchema('');
      setTargetSession(null);
      return;
    }
    let cancelled = false;
    const cfg = connections.find((c) => c.id === targetId);
    (async () => {
      try {
        const { databases } = await listDatabasesDedicated(targetId, cfg?.database);
        if (cancelled) return;
        setTargetDatabases(databases);
        const preferred = cfg?.database ?? '';
        setTargetDatabase((prev) =>
          databases.includes(preferred)
            ? preferred
            : prev && databases.includes(prev)
              ? prev
              : (databases[0] ?? ''),
        );
      } catch (e) {
        if (!cancelled) {
          setTargetDatabases([]);
          setErrorMsg(
            `${t('sync.loadDatabasesFailed')}${e instanceof Error ? e.message : String(e)}`,
          );
          setErrorOpen(true);
        }
      }
    })();
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [targetId, connections]);

  useEffect(() => {
    if (!sourceId || !sourceDatabase) {
      setSourceSession(null);
      setSourceSessionError('');
      return;
    }
    let cancelled = false;
    (async () => {
      try {
        const next = await ensureDedicatedSession(sourceSession, sourceId, sourceDatabase);
        if (!cancelled) {
          setSourceSession(next);
          setSourceSessionError('');
        }
      } catch (e) {
        if (!cancelled) {
          setSourceSession(null);
          setSourceSessionError(
            `${t('sync.sessionFailed')}${e instanceof Error ? e.message : String(e)}`,
          );
        }
      }
    })();
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- reconnect when endpoint or catalog changes
  }, [sourceId, sourceDatabase, t]);

  useEffect(() => {
    if (!targetId || !targetDatabase) {
      setTargetSession(null);
      setTargetSessionError('');
      return;
    }
    let cancelled = false;
    (async () => {
      try {
        const next = await ensureDedicatedSession(targetSession, targetId, targetDatabase);
        if (!cancelled) {
          setTargetSession(next);
          setTargetSessionError('');
        }
      } catch (e) {
        if (!cancelled) {
          setTargetSession(null);
          setTargetSessionError(
            `${t('sync.sessionFailed')}${e instanceof Error ? e.message : String(e)}`,
          );
        }
      }
    })();
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- reconnect when endpoint or catalog changes
  }, [targetId, targetDatabase, t]);

  useEffect(() => {
    const sourceMeta = sourceConn ? DB_REGISTRY[sourceConn.databaseType] : undefined;
    if (
      !sourceId ||
      !sourceDatabase ||
      sourceMeta?.supportsTables !== true ||
      sourceMeta.supportsSQL !== true
    ) {
      setSourceSchemas([]);
      setSourceSchema('');
      return;
    }
    const connId = sourceSession?.dbSessionId;
    if (
      !connId ||
      sourceSession.connectionId !== sourceId ||
      sourceSession.database !== sourceDatabase
    ) {
      return;
    }
    let cancelled = false;
    (async () => {
      try {
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
  }, [sourceId, sourceDatabase, sourceConn?.databaseType, sourceSession]);

  useEffect(() => {
    const targetMeta = targetConn ? DB_REGISTRY[targetConn.databaseType] : undefined;
    if (
      !targetId ||
      !targetDatabase ||
      targetMeta?.supportsTables !== true ||
      targetMeta.supportsSQL !== true
    ) {
      setTargetSchemas([]);
      setTargetSchema('');
      return;
    }
    const connId = targetSession?.dbSessionId;
    if (
      !connId ||
      targetSession.connectionId !== targetId ||
      targetSession.database !== targetDatabase
    ) {
      return;
    }
    let cancelled = false;
    (async () => {
      try {
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
  }, [targetId, targetDatabase, targetConn?.databaseType, targetSession]);

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

  const handleSourceChange = useCallback(
    (id: string) => {
      setSourceId(id);
      resetCompareState();
    },
    [resetCompareState],
  );

  const handleTargetChange = useCallback(
    (id: string) => {
      setTargetId(id);
      resetCompareState();
    },
    [resetCompareState],
  );

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

  const handleInspect = useCallback(async (): Promise<boolean> => {
    if (!validateEndpoints()) return false;

    const generation = ++compareGenerationRef.current;
    setSyncState('inspecting');
    setSelectedTableKey(null);
    setStatusMsg('');
    setInspectionComplete(false);

    try {
      const { source, target } = await refreshEndpointSessions();
      if (generation !== compareGenerationRef.current) return false;
      const srcConnId = source?.dbSessionId;
      const tgtConnId = target?.dbSessionId;
      if (!srcConnId || !tgtConnId) {
        setSyncState('idle');
        return false;
      }

      const inspected = await syncCommands.inspectDataSync(
        srcConnId,
        tgtConnId,
        sourceDatabase,
        targetDatabase,
        sourceSchema || undefined,
        targetSchema || undefined,
      );
      if (generation !== compareGenerationRef.current) return false;
      const withDisabled = markDisabledTables(inspected, disabledTables);
      setMappingResults(withDisabled);
      setInspectionComplete(true);
      setSyncState('idle');
      return true;
    } catch (e) {
      if (generation !== compareGenerationRef.current) return false;
      setErrorMsg(e instanceof Error ? e.message : String(e));
      setErrorOpen(true);
      setSyncState('idle');
      return false;
    }
  }, [
    validateEndpoints,
    refreshEndpointSessions,
    sourceId,
    targetId,
    sourceDatabase,
    targetDatabase,
    sourceSchema,
    targetSchema,
    disabledTables,
  ]);

  const handleCompare = useCallback(async (): Promise<boolean> => {
    if (!validateEndpoints()) return false;
    if (!inspectionComplete) {
      const inspected = await handleInspect();
      if (!inspected) return false;
    }

    const generation = ++compareGenerationRef.current;
    setSyncState('comparing');
    setSelectedTableKey(null);
    setStatusMsg('');
    const jobId = crypto.randomUUID();
    jobIdRef.current = jobId;

    try {
      const { source, target } = await refreshEndpointSessions();
      if (generation !== compareGenerationRef.current) return false;
      const srcConnId = source?.dbSessionId;
      const tgtConnId = target?.dbSessionId;
      if (!srcConnId || !tgtConnId) {
        setSyncState('idle');
        return false;
      }

      const toCompare = tablesForCompare(mappingResults);
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

      if (generation !== compareGenerationRef.current) return false;
      const merged = mergeCompareIntoMappings(mappingResults, compared).map((row) => {
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
      return true;
    } catch (e) {
      if (generation !== compareGenerationRef.current) return false;
      setErrorMsg(e instanceof Error ? e.message : String(e));
      setErrorOpen(true);
      setSyncState('idle');
      return false;
    }
  }, [
    validateEndpoints,
    inspectionComplete,
    handleInspect,
    refreshEndpointSessions,
    mappingResults,
    sourceDatabase,
    targetDatabase,
    sourceSchema,
    targetSchema,
    syncOptions,
  ]);

  const handleCancel = useCallback(async () => {
    const jobId = jobIdRef.current;
    if (jobId) {
      await syncCommands.cancelDataSync(jobId);
      jobIdRef.current = null;
    }
    compareGenerationRef.current += 1;
    setSyncState(mappingResults.length > 0 ? 'compared' : 'idle');
    setStatusMsg(t('sync.compareCancelled'));
  }, [mappingResults.length, t]);

  const toggleDisabledTable = useCallback((sourceTable: string) => {
    setSyncState('idle');
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
      const { source, target } = await refreshEndpointSessions();
      const srcConnId = source?.dbSessionId;
      const tgtConnId = target?.dbSessionId;
      if (!srcConnId || !tgtConnId) {
        setSyncState('compared');
        return;
      }

      const tablesWithSelection = mappingResults.filter(
        (r) => r.status === 'MATCHED' && selectedRowCount(r, syncOptions) > 0,
      );

      let executed = false;
      let usedApplyFallback = false;
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
            setErrorMsg(t('sync.rolledBack'));
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
        usedApplyFallback = true;
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
          setErrorMsg(t('sync.rolledBack'));
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
      setStep('result');
      if (usedApplyFallback) {
        setStatusMsg(t('sync.applyFallbackUsed'));
      } else {
        setStatusMsg('');
      }
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
    refreshEndpointSessions,
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
  const compareDisabled = Boolean(sourceSessionError || targetSessionError);
  const stepIndex = STEPS.indexOf(step);
  const canNext = useMemo(() => {
    switch (step) {
      case 'endpoints':
        return Boolean(
          sourceId &&
            targetId &&
            sourceDatabase &&
            targetDatabase &&
            activePairing?.supported &&
            !compareDisabled,
        );
      case 'setup':
        return !busy;
      case 'objects':
        return inspectionComplete && tablesForCompare(mappingResults).length > 0 && !busy;
      case 'compare':
        return compared && !busy;
      default:
        return false;
    }
  }, [
    step,
    sourceId,
    targetId,
    sourceDatabase,
    targetDatabase,
    activePairing,
    compareDisabled,
    busy,
    inspectionComplete,
    mappingResults,
    compared,
  ]);

  const goNext = useCallback(async () => {
    const next = STEPS[stepIndex + 1];
    if (!next) return;

    if (step === 'setup' && next === 'objects') {
      setStep(next);
      const ok = await handleInspect();
      if (!ok) setStep(step);
      return;
    }
    if (step === 'objects' && next === 'compare') {
      setStep(next);
      const ok = await handleCompare();
      if (!ok) setStep(step);
      return;
    }
    setStep(next);
  }, [step, stepIndex, handleInspect, handleCompare]);

  const goBack = useCallback(() => {
    if (busy) return;
    const previous = STEPS[stepIndex - 1];
    if (previous) setStep(previous);
  }, [busy, stepIndex]);
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
        const connectionId = sourceSession?.dbSessionId ?? targetSession?.dbSessionId;
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
    sourceSession,
    targetSession,
    sourceDatabase,
    targetDatabase,
    t,
  ]);

  const handleReCompare = useCallback(() => {
    setStep('compare');
    void handleCompare();
  }, [handleCompare]);

  return (
    <div
      data-testid="data-sync-window"
      data-sync-state={syncState}
      data-sync-step={step}
      className="flex h-screen min-h-0 flex-col bg-surface text-fg"
    >
      <TitleBar title={t('common.dataSyncTitle')} />

      <div className="border-b border-edge px-6 py-3">
        <div className="mx-auto flex max-w-4xl flex-wrap items-center justify-center gap-1">
          {STEPS.map((s, i) => (
            <div key={s} className="flex items-center gap-1">
              {i > 0 && <ChevronRight className="h-3 w-3 shrink-0 text-fg-muted" aria-hidden />}
              <span
                data-testid={`data-sync-step-${s}`}
                className={cn(
                  'flex items-center gap-1.5 rounded-full px-2 py-0.5 text-xs',
                  i === stepIndex
                    ? 'font-semibold text-accent'
                    : i < stepIndex
                      ? 'text-accent/80'
                      : 'text-fg-muted',
                )}
              >
                <span
                  className={cn(
                    'flex h-5 w-5 items-center justify-center rounded-full text-[10px] font-semibold',
                    i === stepIndex
                      ? 'bg-accent text-white'
                      : i < stepIndex
                        ? 'bg-accent/20 text-accent'
                        : 'bg-surface-raised text-fg-muted',
                  )}
                >
                  {i + 1}
                </span>
                {t(`sync.step.${s}`)}
              </span>
            </div>
          ))}
        </div>
      </div>

      <div className="flex min-h-0 flex-1 flex-col overflow-auto">
        <div
          className={cn(
            'mx-auto flex min-h-0 w-full flex-1 flex-col px-6 py-6',
            NARROW_STEPS.includes(step) ? 'max-w-2xl' : 'max-w-6xl',
          )}
        >
          {step === 'endpoints' && (
            <EndpointsBar
              layout="grid"
              showSwap={false}
              showCompare={false}
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
              compareDisabled={compareDisabled}
              sourceSessionError={sourceSessionError}
              targetSessionError={targetSessionError}
              targetReadOnly={targetReadOnly}
              onSourceChange={handleSourceChange}
              onTargetChange={handleTargetChange}
              onSourceDatabaseChange={handleSourceDatabaseChange}
              onTargetDatabaseChange={handleTargetDatabaseChange}
              onSourceSchemaChange={handleSourceSchemaChange}
              onTargetSchemaChange={handleTargetSchemaChange}
              onSwap={handleSwap}
              onCompare={() => void handleCompare()}
            />
          )}

          {step === 'setup' && (
            <div className="space-y-4 rounded-lg border border-edge bg-surface-alt p-4">
              <div>
                <p className="text-sm font-medium text-fg">{t('sync.optionsTitle')}</p>
                <p className="mt-1 text-xs text-fg-muted">{t('sync.optionsHint')}</p>
              </div>
              <OptionsBar
                options={syncOptions}
                onChange={handleOptionsChange}
                onEnableDelete={handleEnableDelete}
              />
            </div>
          )}

          {step === 'objects' && (
            <div data-testid="data-sync-objects-step" className="space-y-3">
              {syncState === 'inspecting' ? (
                <div className="flex justify-center py-12 text-sm text-fg-muted">
                  <Loader2 className="mr-2 h-5 w-5 animate-spin text-accent" />
                  {t('sync.inspecting')}
                </div>
              ) : mappingResults.length > 0 ? (
                <MappingPanel
                  rows={mappingResults}
                  disabledTables={disabledTables}
                  compared={false}
                  onToggleDisabled={toggleDisabledTable}
                  onOpenSchemaDiff={openSchemaDiffWindow}
                  onOpenDataTransfer={openDataTransferWindow}
                />
              ) : (
                <div className="rounded-lg border border-edge bg-surface-alt px-4 py-8 text-center text-sm text-fg-muted">
                  {t('sync.noTablesFound')}
                </div>
              )}
            </div>
          )}

          {step === 'compare' && (
            <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
              {syncState === 'comparing' ? (
                <div className="flex flex-1 items-center justify-center gap-2 text-sm text-fg-muted">
                  <Loader2 className="h-4 w-4 animate-spin" />
                  {t('sync.comparing')}
                  <Button
                    variant="ghost"
                    size="sm"
                    data-testid="data-sync-cancel"
                    onClick={() => void handleCancel()}
                  >
                    {t('common.cancel')}
                  </Button>
                </div>
              ) : compared ? (
                <>
                  <CompareSummary
                    stats={compareStats}
                    onCopyReport={handleCopyCompareReport}
                    onExplainDiff={handleExplainDiff}
                    explainLoading={explainLoading}
                  />
                  <div className="flex min-h-0 flex-1 rounded-lg border border-edge">
                    <TableListPanel
                      rows={mappingResults}
                      filter={tableFilter}
                      search={tableSearch}
                      selectedTableKey={selectedTableKey}
                      onFilterChange={setTableFilter}
                      onSearchChange={setTableSearch}
                      onSelectTable={setSelectedTableKey}
                    />
                    <div className="flex min-h-0 min-w-0 flex-[1.4] flex-col">
                      {selectedTable ? (
                        <DiffDetail
                          table={selectedTable}
                          options={syncOptions}
                          onUpdateRows={(rows) => updateTableRows(tableKey(selectedTable), rows)}
                        />
                      ) : (
                        <div className="flex flex-1 items-center justify-center text-sm text-fg-muted">
                          {t('sync.selectTableForDetail')}
                        </div>
                      )}
                    </div>
                  </div>
                </>
              ) : (
                <div className="flex flex-1 items-center justify-center text-sm text-fg-muted">
                  {t('sync.comparePrompt')}
                </div>
              )}
            </div>
          )}

          {step === 'preview' && (
            <div className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border border-edge">
              {sourceSession?.dbSessionId && targetSession?.dbSessionId ? (
                <SqlPreview
                  sourceConnId={sourceSession.dbSessionId}
                  targetConnId={targetSession.dbSessionId}
                  sourceDatabase={sourceDatabase}
                  targetDatabase={targetDatabase}
                  sourceSchema={sourceSchema}
                  targetSchema={targetSchema}
                  tables={mappingResults}
                  options={syncOptions}
                />
              ) : (
                <div
                  data-testid="data-sync-preview-session-required"
                  className="flex flex-1 items-center justify-center text-sm text-fg-muted"
                >
                  {t('sync.sessionRequired')}
                </div>
              )}
            </div>
          )}

          {step === 'result' && (
            <div
              data-testid="data-sync-result"
              className="space-y-4 rounded-lg border border-edge bg-surface-alt p-6"
            >
              <div
                data-testid="data-sync-execute-done"
                className="flex flex-wrap items-center gap-3 text-sm text-green-700 dark:text-green-400"
              >
                <span>{t('sync.executeDone')}</span>
                <Button
                  variant="secondary"
                  size="sm"
                  data-testid="data-sync-re-compare"
                  onClick={handleReCompare}
                >
                  {t('sync.reCompare')}
                </Button>
              </div>
              <CompareSummary
                stats={compareStats}
                onCopyReport={handleCopyCompareReport}
                onExplainDiff={handleExplainDiff}
                explainLoading={explainLoading}
              />
            </div>
          )}
        </div>
      </div>

      {step === 'preview' && (
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

      <div className="flex shrink-0 items-center justify-between border-t border-edge px-6 py-3">
        <Button
          variant="ghost"
          data-testid="data-sync-back"
          disabled={stepIndex === 0 || busy}
          onClick={goBack}
        >
          <ChevronLeft className="h-4 w-4" /> {t('transfer.back')}
        </Button>
        {step !== 'preview' && step !== 'result' ? (
          <Button
            data-testid="data-sync-next"
            disabled={!canNext || busy}
            onClick={() => void goNext()}
          >
            {busy ? <Loader2 className="h-4 w-4 animate-spin" /> : null}
            {t('transfer.next')}
            <ChevronRight className="h-4 w-4" />
          </Button>
        ) : null}
      </div>

      <StatusBar left={<span className="truncate">{statusMsg || t('common.dataSync')}</span>} />

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
              variant="run"
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
