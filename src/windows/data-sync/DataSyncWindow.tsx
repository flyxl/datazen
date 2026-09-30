import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { ChevronLeft, ChevronRight } from 'lucide-react';
import { TitleBar } from '../../components/TitleBar';
import { StatusBar } from '../../components/StatusBar';
import { LocaleDomainLoading } from '../../components/LocaleDomainLoading';
import { Button } from '../../components/ui/Button';
import { Dialog } from '../../components/ui/Dialog';
import { Select } from '../../components/ui/Select';
import { CopyableError } from '../../components/ui/CopyableError';
import { Input } from '../../components/ui/Input';
import { Spinner } from '../../components/ui/Spinner';
import { aiCommands } from '../../commands/ai';
import {
  syncCommands,
  DEFAULT_SYNC_OPTIONS,
  type DataSyncExecutionResult,
  type DataSyncRowChange,
  type DataSyncSelectedRow,
  type DataSyncSelectionExclusion,
  type DataSyncTableSelection,
  type DataSyncSourceFilter,
  type DataSyncTableMapping,
  type DataSyncTableResult,
  type SyncProfile,
  type SyncOptions,
} from '../../commands/sync';
import { databaseCommands } from '../../commands/database';
import { useI18n } from '../../hooks/useI18n';
import { useLocaleDomains } from '../../hooks/useLocaleDomains';
import { useSettings } from '../../hooks/useSettings';
import { useSettingsStore } from '../../stores/settingsStore';
import { useAiStore } from '../../stores/aiStore';
import { listenCrossWindow } from '../../lib/crossWindowBus';
import { cn } from '../../lib/cn';
import { openDataTransferWindow, openSchemaDiffWindow } from '../../lib/windowManager';
import {
  pickPrefillDatabase,
  pickPrefillSchema,
  resolveDefaultDatabase,
  useMigrationEndpointPrefill,
} from '../../lib/migrationWindowPrefill';
import {
  ensureDedicatedSession,
  listDatabasesDedicated,
  releaseDedicatedSession,
  type DedicatedSideSession,
} from '../../lib/dedicatedDbSession';
import { useSyncPairingState } from '../../lib/syncPairing';
import { isVerifiedMigrationPair } from '../../lib/migrationVerification';
import { DB_REGISTRY } from '../../lib/databaseTypes';
import { MigrationRunHistoryDialog } from '../../components/migration/MigrationRunHistoryDialog';
import type { MigrationRunRecord } from '../../commands/history';
import type { ConnectionConfig } from '../../types';
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
  rowKeyString,
  defaultRowSelected,
  rowDiffCounts,
  summarizeCompare,
  tableHasRowDiffs,
  tableKey,
  tablesForCompare,
} from './mappingView';
import { buildCompareReportText } from './compareReport';

import {
  useDataSyncWizardState,
  WIZARD_STEPS,
  NARROW_WIZARD_STEPS,
} from './useDataSyncWizardState';

function selectedRowToken(
  row: Pick<DataSyncSelectedRow, 'sourceTable' | 'targetTable' | 'operation' | 'key'>,
): string {
  return `${row.sourceTable}\u0000${row.targetTable}\u0000${row.operation}\u0000${rowKeyString(row.key)}`;
}

function scopeSelectsOperation(
  scope: DataSyncTableSelection,
  operation: DataSyncRowChange['operation'],
  options: SyncOptions,
): boolean {
  return (
    operation !== 'UNCHANGED' &&
    scope.operations.includes(operation) &&
    (scope.selectionMode === 'all' || defaultRowSelected(operation, options))
  );
}

function scopeForOperation(
  scopes: DataSyncTableSelection[],
  table: Pick<DataSyncTableResult, 'sourceTable' | 'targetTable'>,
  operation: DataSyncRowChange['operation'],
): DataSyncTableSelection | undefined {
  return scopes.find(
    (scope) =>
      scope.sourceTable === table.sourceTable &&
      scope.targetTable === table.targetTable &&
      scope.operations.includes(operation as Exclude<DataSyncRowChange['operation'], 'UNCHANGED'>),
  );
}

export function DataSyncWindow() {
  const localesReady = useLocaleDomains(['sync']);
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
  const [syncProfiles, setSyncProfiles] = useState<SyncProfile[]>([]);
  const [profileName, setProfileName] = useState('');
  const [selectedProfileId, setSelectedProfileId] = useState('');
  const {
    syncOptions,
    setSyncOptions,
    mappingResults,
    setMappingResults,
    disabledTables,
    setDisabledTables,
    step,
    setStep,
    inspectionComplete,
    setInspectionComplete,
    syncState,
    setSyncState,
    selectedTableKey,
    setSelectedTableKey,
    tableFilter,
    setTableFilter,
    tableSearch,
    setTableSearch,
    resetCompareState,
  } = useDataSyncWizardState();
  const [errorMsg, setErrorMsg] = useState('');
  const [errorOpen, setErrorOpen] = useState(false);
  const [statusMsg, setStatusMsg] = useState('');
  const [executeProgress, setExecuteProgress] = useState('');
  const [sourceSessionError, setSourceSessionError] = useState('');
  const [targetSessionError, setTargetSessionError] = useState('');
  const [deleteConfirmOpen, setDeleteConfirmOpen] = useState(false);
  const [executeConfirmOpen, setExecuteConfirmOpen] = useState(false);
  const [explainOpen, setExplainOpen] = useState(false);
  const [explainLoading, setExplainLoading] = useState(false);
  const [explainText, setExplainText] = useState('');
  const [writeOutcomeUncertain, setWriteOutcomeUncertain] = useState(false);
  const [scopeReconfirmationRequired, setScopeReconfirmationRequired] = useState(false);
  const [lastExecutionResult, setLastExecutionResult] = useState<DataSyncExecutionResult | null>(
    null,
  );
  const [selectedRows, setSelectedRows] = useState<DataSyncSelectedRow[]>([]);
  const [tableSelections, setTableSelections] = useState<DataSyncTableSelection[]>([]);
  const [pageLoading, setPageLoading] = useState(false);
  const [pageIndex, setPageIndex] = useState<Record<string, number>>({});
  const [pageCursors, setPageCursors] = useState<
    Record<string, { current: string | null; next: string | null; previous: string[] }>
  >({});
  const jobIdRef = useRef<string | null>(null);
  const jobKindRef = useRef<'compare' | 'execute' | null>(null);
  const cancelRequestedJobRef = useRef<string | null>(null);
  const cancellingStatusJobRef = useRef<string | null>(null);
  const compareGenerationRef = useRef(0);
  const writeInFlightRef = useRef(false);
  const syncStateRef = useRef(syncState);
  const selectedRowsRef = useRef<DataSyncSelectedRow[]>([]);
  const tableSelectionsRef = useRef<DataSyncTableSelection[]>([]);
  const loadedPageRef = useRef<Set<string>>(new Set());
  const pendingProfileMappingsRef = useRef<DataSyncTableMapping[] | null>(null);
  selectedRowsRef.current = selectedRows;
  tableSelectionsRef.current = tableSelections;
  syncStateRef.current = syncState;

  useEffect(() => {
    setSelectedRows([]);
    setTableSelections([]);
    setPageIndex({});
    setPageCursors({});
    loadedPageRef.current.clear();
  }, [sourceId, targetId, sourceDatabase, targetDatabase, sourceSchema, targetSchema]);

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

  const migrationPrefillRef = useMigrationEndpointPrefill(connections, setSourceId, setTargetId);

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
    const experimentalHint = t('common.experimentalPairHint');
    const srcType = sourceConn?.databaseType;
    return connections.map((c) => {
      const unsupported = Boolean(
        srcType && Object.hasOwn(targetSupport, c.id) && !targetSupport[c.id],
      );
      const base = `${c.name} (${c.databaseType})`;
      const experimental = Boolean(
        srcType &&
          !unsupported &&
          Object.hasOwn(targetSupport, c.id) &&
          !isVerifiedMigrationPair(srcType, c.databaseType),
      );
      return {
        value: c.id,
        label: unsupported
          ? `${base} — ${hint}`
          : experimental
            ? `${base} — ${experimentalHint}`
            : base,
        disabled: unsupported,
        title: unsupported ? hint : experimental ? experimentalHint : undefined,
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
        setSourceDatabases(databases ?? []);
        const preferred = cfg?.database ?? '';
        setSourceDatabase((prev) =>
          pickPrefillDatabase(
            migrationPrefillRef,
            'source',
            databases,
            (current) => resolveDefaultDatabase(databases, preferred, current),
            prev,
          ),
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
        setTargetDatabases(databases ?? []);
        const preferred = cfg?.database ?? '';
        setTargetDatabase((prev) =>
          pickPrefillDatabase(
            migrationPrefillRef,
            'target',
            databases,
            (current) => resolveDefaultDatabase(databases, preferred, current),
            prev,
          ),
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
        setSourceSchema((prev) =>
          pickPrefillSchema(
            migrationPrefillRef,
            'source',
            schemas,
            (current) => pickDefaultSchema(schemas, current),
            prev,
          ),
        );
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
        setTargetSchema((prev) =>
          pickPrefillSchema(
            migrationPrefillRef,
            'target',
            schemas,
            (current) => pickDefaultSchema(schemas, current),
            prev,
          ),
        );
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
    pendingProfileMappingsRef.current = null;
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
      pendingProfileMappingsRef.current = null;
      setSourceId(id);
      resetCompareState();
    },
    [resetCompareState],
  );

  const handleTargetChange = useCallback(
    (id: string) => {
      pendingProfileMappingsRef.current = null;
      setTargetId(id);
      resetCompareState();
    },
    [resetCompareState],
  );

  const handleSourceDatabaseChange = useCallback(
    (db: string) => {
      pendingProfileMappingsRef.current = null;
      setSourceDatabase(db);
      resetCompareState();
    },
    [resetCompareState],
  );

  const handleTargetDatabaseChange = useCallback(
    (db: string) => {
      pendingProfileMappingsRef.current = null;
      setTargetDatabase(db);
      resetCompareState();
    },
    [resetCompareState],
  );

  const handleSourceSchemaChange = useCallback(
    (schema: string) => {
      pendingProfileMappingsRef.current = null;
      setSourceSchema(schema);
      resetCompareState();
    },
    [resetCompareState],
  );

  const handleTargetSchemaChange = useCallback(
    (schema: string) => {
      pendingProfileMappingsRef.current = null;
      setTargetSchema(schema);
      resetCompareState();
    },
    [resetCompareState],
  );

  const loadSyncProfiles = useCallback(() => {
    void syncCommands
      .getSyncProfiles()
      .then(setSyncProfiles)
      .catch(() => setSyncProfiles([]));
  }, []);

  const currentProfileTables = useCallback((): DataSyncTableMapping[] => {
    if (mappingResults.length === 0) return pendingProfileMappingsRef.current ?? [];
    return mappingResults
      .filter((row) => row.sourceTable.trim() && row.targetTable.trim())
      .map((row) => ({
        sourceTable: row.sourceTable,
        targetTable: row.targetTable,
        enabled: !disabledTables.has(row.sourceTable) && row.status !== 'DISABLED',
        sourceFilter: row.sourceFilter,
      }));
  }, [disabledTables, mappingResults]);

  const saveCurrentProfile = useCallback(async () => {
    const name = profileName.trim();
    if (!name || !sourceId || !targetId) {
      setErrorMsg(t('sync.profile.missingFields'));
      setErrorOpen(true);
      return;
    }
    const existing = syncProfiles.find((profile) => profile.id === selectedProfileId);
    const now = new Date().toISOString();
    const profile: SyncProfile = {
      version: 1,
      id: existing?.id ?? crypto.randomUUID(),
      name,
      sourceConnectionId: sourceId,
      targetConnectionId: targetId,
      sourceDatabase: sourceDatabase || null,
      targetDatabase: targetDatabase || null,
      sourceSchema: sourceSchema || null,
      targetSchema: targetSchema || null,
      tables: currentProfileTables(),
      options: syncOptions,
      createdAt: existing?.createdAt ?? now,
      updatedAt: now,
    };
    try {
      await syncCommands.saveSyncProfile(profile);
      setSelectedProfileId(profile.id);
      setProfileName(profile.name);
      loadSyncProfiles();
      setStatusMsg(t('sync.profile.saved'));
    } catch (error) {
      setErrorMsg(error instanceof Error ? error.message : String(error));
      setErrorOpen(true);
    }
  }, [
    currentProfileTables,
    loadSyncProfiles,
    profileName,
    selectedProfileId,
    sourceDatabase,
    sourceId,
    sourceSchema,
    syncOptions,
    syncProfiles,
    t,
    targetDatabase,
    targetId,
    targetSchema,
  ]);

  const loadSelectedProfile = useCallback(() => {
    const profile = syncProfiles.find((candidate) => candidate.id === selectedProfileId);
    if (!profile) return;
    const sourceExists = connections.some(
      (connection) => connection.id === profile.sourceConnectionId,
    );
    const targetExists = connections.some(
      (connection) => connection.id === profile.targetConnectionId,
    );
    if (!sourceExists || !targetExists) {
      setErrorMsg(t('sync.profile.missingConnection'));
      setErrorOpen(true);
      return;
    }
    pendingProfileMappingsRef.current = profile.tables;
    setProfileName(profile.name);
    setSourceId(profile.sourceConnectionId);
    setTargetId(profile.targetConnectionId);
    setSourceDatabase(profile.sourceDatabase ?? '');
    setTargetDatabase(profile.targetDatabase ?? '');
    setSourceSchema(profile.sourceSchema ?? '');
    setTargetSchema(profile.targetSchema ?? '');
    setSyncOptions(profile.options);
    resetCompareState();
    setSelectedRows([]);
    setTableSelections([]);
    setStep('endpoints');
    setStatusMsg(t('sync.profile.loaded'));
  }, [connections, resetCompareState, selectedProfileId, setSyncOptions, setStep, syncProfiles, t]);

  const deleteSelectedProfile = useCallback(async () => {
    if (!selectedProfileId) return;
    try {
      await syncCommands.deleteSyncProfile(selectedProfileId);
      setSelectedProfileId('');
      setProfileName('');
      loadSyncProfiles();
      setStatusMsg(t('sync.profile.deleted'));
    } catch (error) {
      setErrorMsg(error instanceof Error ? error.message : String(error));
      setErrorOpen(true);
    }
  }, [loadSyncProfiles, selectedProfileId, t]);

  const reconcileUnknownRun = useCallback(
    async (run: MigrationRunRecord): Promise<boolean> => {
      if (run.operation !== 'dataSync' || run.outcome !== 'unknown') return false;
      if (writeInFlightRef.current) {
        setErrorMsg(t('migrationHistory.syncReconcileBusy'));
        setErrorOpen(true);
        return false;
      }

      const [latestProfiles, latestConnections] = await Promise.all([
        syncCommands.getSyncProfiles(),
        invoke<ConnectionConfig[]>('get_connections'),
      ]);
      setSyncProfiles(latestProfiles);
      setConnections(latestConnections);
      const savedProfile = run.profileId
        ? latestProfiles.find((profile) => profile.id === run.profileId)
        : undefined;
      const sameRevision =
        !!savedProfile &&
        !!run.profileRevision &&
        Date.parse(savedProfile.updatedAt) === Date.parse(run.profileRevision);
      const reusableProfile =
        sameRevision &&
        (!run.sourceConnectionId || run.sourceConnectionId === savedProfile.sourceConnectionId) &&
        (!run.targetConnectionId || run.targetConnectionId === savedProfile.targetConnectionId)
          ? savedProfile
          : undefined;
      const sourceConnectionId =
        run.sourceConnectionId ?? reusableProfile?.sourceConnectionId ?? '';
      const targetConnectionId =
        run.targetConnectionId ?? reusableProfile?.targetConnectionId ?? '';
      const connectionIds = new Set(latestConnections.map((connection) => connection.id));
      if (
        (sourceConnectionId && !connectionIds.has(sourceConnectionId)) ||
        (targetConnectionId && !connectionIds.has(targetConnectionId))
      ) {
        setErrorMsg(t('migrationHistory.syncReconcileMissingConnection'));
        setErrorOpen(true);
        return false;
      }

      let profileScopeCanBeRestored = Boolean(reusableProfile);
      let sourceDatabasesForRestore: string[] = [];
      let targetDatabasesForRestore: string[] = [];
      if (reusableProfile) {
        try {
          const [sourceCatalog, targetCatalog] = await Promise.all([
            listDatabasesDedicated(sourceConnectionId),
            listDatabasesDedicated(targetConnectionId),
          ]);
          sourceDatabasesForRestore = sourceCatalog.databases ?? [];
          targetDatabasesForRestore = targetCatalog.databases ?? [];
          profileScopeCanBeRestored = Boolean(
            reusableProfile.sourceDatabase &&
              reusableProfile.targetDatabase &&
              sourceDatabasesForRestore.includes(reusableProfile.sourceDatabase) &&
              targetDatabasesForRestore.includes(reusableProfile.targetDatabase),
          );
        } catch {
          profileScopeCanBeRestored = false;
        }
      }

      compareGenerationRef.current += 1;
      jobIdRef.current = null;
      jobKindRef.current = null;
      cancelRequestedJobRef.current = null;
      const restoredProfile = profileScopeCanBeRestored ? reusableProfile : undefined;
      pendingProfileMappingsRef.current = restoredProfile?.tables ?? null;
      setSelectedProfileId(restoredProfile?.id ?? '');
      setProfileName(restoredProfile?.name ?? '');
      setSourceId(sourceConnectionId);
      setTargetId(targetConnectionId);
      setSourceDatabase(restoredProfile?.sourceDatabase ?? '');
      setTargetDatabase(restoredProfile?.targetDatabase ?? '');
      setSourceSchema(restoredProfile?.sourceSchema ?? '');
      setTargetSchema(restoredProfile?.targetSchema ?? '');
      setSourceDatabases(sourceDatabasesForRestore);
      setTargetDatabases(targetDatabasesForRestore);
      setSourceSchemas([]);
      setTargetSchemas([]);
      setSyncOptions(restoredProfile?.options ?? DEFAULT_SYNC_OPTIONS);
      resetCompareState();
      setSelectedRows([]);
      setTableSelections([]);
      setPageIndex({});
      setPageCursors({});
      loadedPageRef.current.clear();
      setLastExecutionResult(null);
      setWriteOutcomeUncertain(true);
      setScopeReconfirmationRequired(!restoredProfile);
      setSyncState('unknown');
      setStep('endpoints');
      setStatusMsg(
        restoredProfile
          ? t('migrationHistory.syncReconcileProfileReady')
          : reusableProfile
            ? t('migrationHistory.syncReconcileScopeUnavailable')
            : run.profileId
              ? t('migrationHistory.syncReconcileProfileChanged')
              : t('migrationHistory.syncReconcileNeedsScope'),
      );
      return true;
    },
    [resetCompareState, setStep, setSyncOptions, t],
  );

  useEffect(() => {
    loadSyncProfiles();
  }, [loadSyncProfiles]);

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
        setSyncState(writeOutcomeUncertain ? 'unknown' : 'idle');
        return false;
      }

      const profileMappings = pendingProfileMappingsRef.current;
      const profileDisabled = profileMappings
        ? new Set(
            profileMappings
              .filter((mapping) => !mapping.enabled)
              .map((mapping) => mapping.sourceTable),
          )
        : disabledTables;

      const inspected = profileMappings
        ? await syncCommands.inspectDataSync(
            srcConnId,
            tgtConnId,
            sourceDatabase,
            targetDatabase,
            sourceSchema || undefined,
            targetSchema || undefined,
            profileMappings,
          )
        : await syncCommands.inspectDataSync(
            srcConnId,
            tgtConnId,
            sourceDatabase,
            targetDatabase,
            sourceSchema || undefined,
            targetSchema || undefined,
          );
      if (generation !== compareGenerationRef.current) return false;
      if (profileMappings) setDisabledTables(profileDisabled);
      const withDisabled = markDisabledTables(inspected, profileDisabled);
      setMappingResults(withDisabled);
      pendingProfileMappingsRef.current = null;
      setInspectionComplete(true);
      setSyncState('idle');
      return true;
    } catch (e) {
      if (generation !== compareGenerationRef.current) return false;
      setErrorMsg(e instanceof Error ? e.message : String(e));
      setErrorOpen(true);
      setSyncState(writeOutcomeUncertain ? 'unknown' : 'idle');
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
    setDisabledTables,
    writeOutcomeUncertain,
  ]);

  const handleCompare = useCallback(async (): Promise<boolean> => {
    if (!validateEndpoints()) return false;
    if (!inspectionComplete) {
      const inspected = await handleInspect();
      if (!inspected) return false;
    }

    const generation = ++compareGenerationRef.current;
    setSyncState('comparing');
    setLastExecutionResult(null);
    setSelectedTableKey(null);
    setSelectedRows([]);
    setTableSelections([]);
    setPageIndex({});
    setPageCursors({});
    loadedPageRef.current.clear();
    setStatusMsg('');
    const jobId = crypto.randomUUID();
    jobIdRef.current = jobId;
    jobKindRef.current = 'compare';
    cancelRequestedJobRef.current = null;

    try {
      const { source, target } = await refreshEndpointSessions();
      if (generation !== compareGenerationRef.current) return false;
      const srcConnId = source?.dbSessionId;
      const tgtConnId = target?.dbSessionId;
      if (!srcConnId || !tgtConnId) {
        setSyncState(writeOutcomeUncertain ? 'unknown' : 'idle');
        return false;
      }

      const toCompare = tablesForCompare(mappingResults);
      const filters = Object.fromEntries(
        mappingResults
          .filter((row) => row.status === 'MATCHED' && row.sourceFilter)
          .map((row) => [row.sourceTable, row.sourceFilter as DataSyncSourceFilter]),
      );
      const comparedResponse = Object.keys(filters).length
        ? await syncCommands.compareDataSync(
            srcConnId,
            tgtConnId,
            toCompare,
            jobId,
            sourceDatabase,
            targetDatabase,
            sourceSchema || undefined,
            targetSchema || undefined,
            syncOptions,
            filters,
          )
        : await syncCommands.compareDataSync(
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
      const compared = Array.isArray(comparedResponse) ? comparedResponse : comparedResponse.tables;
      const pagedComparison =
        !Array.isArray(comparedResponse) && comparedResponse.contractVersion != null;
      if (jobIdRef.current === jobId && jobKindRef.current === 'compare') {
        jobIdRef.current = null;
        jobKindRef.current = null;
      }
      const merged = mergeCompareIntoMappings(mappingResults, compared).map((row) => {
        if (!pagedComparison && row.rows) {
          return { ...row, rows: applyOptionsToRows(row.rows, syncOptions) };
        }
        return { ...row, rows: undefined };
      });
      if (!pagedComparison) {
        setSelectedRows(
          merged.flatMap((table) =>
            (table.rows ?? [])
              .filter((row) => row.selected && row.operation !== 'UNCHANGED')
              .map((row) => ({
                sourceTable: table.sourceTable,
                targetTable: table.targetTable,
                operation: row.operation,
                key: row.key,
              })),
          ),
        );
      }
      setMappingResults(merged);
      setWriteOutcomeUncertain(false);
      const firstDiff = merged.find((r) => r.status === 'MATCHED' && tableHasRowDiffs(r));
      if (firstDiff) setSelectedTableKey(tableKey(firstDiff));
      setSyncState('compared');
      return true;
    } catch (e) {
      if (generation !== compareGenerationRef.current) return false;
      if (jobIdRef.current === jobId && jobKindRef.current === 'compare') {
        jobIdRef.current = null;
        jobKindRef.current = null;
      }
      setErrorMsg(e instanceof Error ? e.message : String(e));
      setErrorOpen(true);
      setSyncState(writeOutcomeUncertain ? 'unknown' : 'idle');
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
    writeOutcomeUncertain,
    setLastExecutionResult,
  ]);

  const handleCancel = useCallback(async () => {
    const cancellationGeneration = ++compareGenerationRef.current;
    // Reassert the fence before yielding so a comparison completion queued in the same tick cannot win.
    if (writeOutcomeUncertain) setWriteOutcomeUncertain(true);
    const jobId = jobIdRef.current;
    const jobKind = jobKindRef.current;
    const cancelledPhase = syncStateRef.current;
    if (jobId) cancelRequestedJobRef.current = jobId;

    if (jobKind === 'execute' && writeInFlightRef.current) {
      cancellingStatusJobRef.current = jobId;
      setStatusMsg(t('sync.cancellingExecution'));
    } else {
      setSyncState(
        writeOutcomeUncertain ? 'unknown' : mappingResults.length > 0 ? 'compared' : 'idle',
      );
      setExecuteProgress('');
      setStatusMsg(t('sync.compareCancelled'));
    }

    if (jobId) {
      await syncCommands.cancelDataSync(jobId);
    }
    if (
      cancellationGeneration !== compareGenerationRef.current ||
      jobIdRef.current !== jobId ||
      jobKindRef.current !== jobKind
    )
      return;
    if (jobKind === 'execute' && syncStateRef.current !== cancelledPhase) return;
    if (jobKind === 'execute' && writeInFlightRef.current) return;
    jobIdRef.current = null;
    jobKindRef.current = null;
    if (cancelRequestedJobRef.current === jobId) cancelRequestedJobRef.current = null;
  }, [mappingResults.length, t, writeOutcomeUncertain]);

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

  const updateSourceFilter = useCallback(
    (sourceTable: string, sourceFilter: DataSyncSourceFilter | undefined) => {
      setSyncState('idle');
      setMappingResults((rows) =>
        rows.map((row) =>
          row.sourceTable === sourceTable ? { ...row, sourceFilter, rows: undefined } : row,
        ),
      );
    },
    [],
  );

  const handleOptionsChange = useCallback(
    (next: SyncOptions) => {
      const previousPolicy = syncOptions.conflictPolicy ?? 'abort';
      const nextPolicy = next.conflictPolicy ?? 'abort';
      setSyncOptions(next);
      if (
        previousPolicy !== nextPolicy &&
        (syncState === 'compared' || step === 'preview' || step === 'result')
      ) {
        // The server binds conflict handling to the immutable comparison plan.
        // Drop row-level comparison state before allowing the user to continue.
        setMappingResults((rows) => rows.map((row) => ({ ...row, rows: undefined })));
        setSelectedTableKey(null);
        setSyncState('idle');
        setStep('setup');
        return;
      }
      setMappingResults((rows) =>
        rows.map((row) => {
          if (!row.rows) return row;
          return { ...row, rows: applyOptionsToRows(row.rows, next) };
        }),
      );
      setSelectedRows((rows) =>
        rows.filter((row) => {
          if (row.operation === 'INSERT') return next.insert;
          if (row.operation === 'UPDATE') return next.update;
          if (row.operation === 'DELETE') return next.delete;
          return false;
        }),
      );
      setTableSelections((scopes) =>
        scopes
          .map((scope) => ({
            ...scope,
            operations: scope.operations.filter((operation) => operationAllowed(operation, next)),
            excludedRows: scope.excludedRows.filter((row) => operationAllowed(row.operation, next)),
          }))
          .filter((scope) => scope.operations.length > 0),
      );
    },
    [
      setMappingResults,
      setSelectedTableKey,
      setStep,
      setSyncOptions,
      setSyncState,
      step,
      syncOptions.conflictPolicy,
      syncState,
    ],
  );

  const handleEnableDelete = useCallback(() => {
    setDeleteConfirmOpen(true);
  }, []);

  const confirmEnableDelete = useCallback(() => {
    setSyncOptions((prev) => ({ ...prev, delete: true }));
    setDeleteConfirmOpen(false);
  }, []);

  const compared =
    syncState === 'compared' ||
    syncState === 'executing' ||
    syncState === 'unknown' ||
    syncState === 'done';

  const selectedTable = useMemo(
    () => mappingResults.find((r) => tableKey(r) === selectedTableKey) ?? null,
    [mappingResults, selectedTableKey],
  );

  const loadTablePage = useCallback(
    async (
      table: NonNullable<typeof selectedTable>,
      direction: 'initial' | 'previous' | 'next' = 'initial',
    ) => {
      if (table.status !== 'MATCHED') return;
      const key = `${table.sourceTable}\u0000${table.targetTable}`;
      const current = pageCursors[key] ?? {
        current: table.firstCursor ?? null,
        next: null,
        previous: [],
      };
      const cursor =
        direction === 'next'
          ? current.next
          : direction === 'previous'
            ? (current.previous[current.previous.length - 1] ?? null)
            : current.current;
      if (direction === 'next' && !current.next) return;
      if (direction === 'previous' && current.previous.length === 0) return;
      setPageLoading(true);
      try {
        const page = await syncCommands.getDataSyncComparisonPage(
          cursor,
          table.sourceTable,
          table.targetTable,
          table.pageSize,
        );
        const pageToken = `${key}\u0000${cursor ?? ''}`;
        const firstLoad = !loadedPageRef.current.has(pageToken);
        loadedPageRef.current.add(pageToken);
        const selectedTokenSet = new Set(
          selectedRowsRef.current
            .filter(
              (row) =>
                row.sourceTable === table.sourceTable && row.targetTable === table.targetTable,
            )
            .map(selectedRowToken),
        );
        const pageRows = page.rows.map((row) => {
          const scope = scopeForOperation(tableSelectionsRef.current, table, row.operation);
          return {
            ...row,
            selected:
              scope && scopeSelectsOperation(scope, row.operation, syncOptions)
                ? !scope.excludedRows.some(
                    (excluded) =>
                      excluded.operation === row.operation &&
                      rowKeyString(excluded.key) === rowKeyString(row.key),
                  )
                : firstLoad
                  ? row.selected
                  : selectedTokenSet.has(
                      selectedRowToken({
                        sourceTable: table.sourceTable,
                        targetTable: table.targetTable,
                        operation: row.operation,
                        key: row.key,
                      }),
                    ),
          };
        });
        if (firstLoad) {
          setSelectedRows((previous) => {
            const known = new Set(previous.map(selectedRowToken));
            const additions = pageRows
              .filter((row) => row.selected && row.operation !== 'UNCHANGED')
              .filter(
                (row) =>
                  !tableSelectionsRef.current.some(
                    (scope) =>
                      scope.sourceTable === table.sourceTable &&
                      scope.targetTable === table.targetTable &&
                      scopeSelectsOperation(scope, row.operation, syncOptions),
                  ),
              )
              .map((row) => ({
                sourceTable: table.sourceTable,
                targetTable: table.targetTable,
                operation: row.operation,
                key: row.key,
              }))
              .filter((row) => !known.has(selectedRowToken(row)));
            return additions.length ? [...previous, ...additions] : previous;
          });
        }
        setMappingResults((previous) =>
          previous.map((row) =>
            tableKey(row) === tableKey(table) ? { ...row, rows: pageRows } : row,
          ),
        );
        const nextPrevious =
          direction === 'next'
            ? [...current.previous, current.current ?? '']
            : direction === 'previous'
              ? current.previous.slice(0, -1)
              : [];
        setPageCursors((previous) => ({
          ...previous,
          [key]: { current: cursor, next: page.nextCursor, previous: nextPrevious },
        }));
        setPageIndex((previous) => ({
          ...previous,
          [key]: Math.max(
            0,
            (previous[key] ?? 0) + (direction === 'next' ? 1 : direction === 'previous' ? -1 : 0),
          ),
        }));
      } catch (error) {
        setErrorMsg(error instanceof Error ? error.message : String(error));
        setErrorOpen(true);
      } finally {
        setPageLoading(false);
      }
    },
    [pageCursors, setMappingResults, syncOptions],
  );

  useEffect(() => {
    if (!compared || !selectedTable || selectedTable.rows || selectedTable.status !== 'MATCHED')
      return;
    void loadTablePage(selectedTable, 'initial');
  }, [compared, selectedTableKey, selectedTable, loadTablePage]);

  const totalSelectedRows = useMemo(() => {
    const scopedCount = tableSelections.reduce((total, scope) => {
      const table = mappingResults.find(
        (row) => row.sourceTable === scope.sourceTable && row.targetTable === scope.targetTable,
      );
      if (!table) return total;
      const counts = rowDiffCounts(table);
      return (
        total +
        scope.operations.reduce((subtotal, operation) => {
          if (scope.selectionMode === 'defaults' && !defaultRowSelected(operation, syncOptions)) {
            return subtotal;
          }
          const count =
            operation === 'INSERT'
              ? counts.inserts
              : operation === 'UPDATE'
                ? counts.updates
                : counts.deletes;
          const excluded = scope.excludedRows.filter((row) => row.operation === operation).length;
          return subtotal + Math.max(0, count - excluded);
        }, 0)
      );
    }, 0);
    const explicitCount = selectedRows.filter((row) => {
      if (!operationAllowed(row.operation, syncOptions)) return false;
      return !tableSelections.some(
        (scope) =>
          scope.sourceTable === row.sourceTable &&
          scope.targetTable === row.targetTable &&
          scopeSelectsOperation(scope, row.operation, syncOptions),
      );
    }).length;
    return scopedCount + explicitCount;
  }, [mappingResults, selectedRows, syncOptions, tableSelections]);

  const hasSelectedDeletes = useMemo(() => {
    return (
      syncOptions.delete &&
      (selectedRows.some((row) => row.operation === 'DELETE') ||
        tableSelections.some((scope) => scopeSelectsOperation(scope, 'DELETE', syncOptions)))
    );
  }, [selectedRows, syncOptions.delete, tableSelections]);

  const runExecute = useCallback(async () => {
    if (!sourceId || !targetId) return;
    if (writeOutcomeUncertain) {
      setErrorMsg(t('sync.executionUnknown'));
      setErrorOpen(true);
      setSyncState('unknown');
      return;
    }
    if (targetReadOnly) {
      setErrorMsg(t('sync.targetReadOnly'));
      setErrorOpen(true);
      return;
    }
    setSyncState('executing');
    setExecuteProgress(t('sync.executing'));
    const jobId = crypto.randomUUID();
    jobIdRef.current = jobId;
    jobKindRef.current = 'execute';
    cancelRequestedJobRef.current = null;
    cancellingStatusJobRef.current = null;

    let writeStarted = false;
    let executionResolved = false;
    try {
      const { source, target } = await refreshEndpointSessions();
      const srcConnId = source?.dbSessionId;
      const tgtConnId = target?.dbSessionId;
      if (!srcConnId || !tgtConnId) {
        setSyncState('compared');
        return;
      }

      const tablesWithSelection = mappingResults.filter(
        (r) =>
          r.status === 'MATCHED' &&
          (selectedRows.some(
            (selected) =>
              selected.sourceTable === r.sourceTable && selected.targetTable === r.targetTable,
          ) ||
            tableSelections.some(
              (scope) => scope.sourceTable === r.sourceTable && scope.targetTable === r.targetTable,
            )),
      );

      const stmts = await syncCommands.generateDataSyncSql(
        srcConnId,
        tgtConnId,
        tablesWithSelection,
        syncOptions,
        sourceDatabase,
        targetDatabase,
        sourceSchema || undefined,
        targetSchema || undefined,
        selectedRows,
        ...(tableSelections.length ? [tableSelections] : []),
      );
      if (jobIdRef.current !== jobId || cancelRequestedJobRef.current === jobId) return;
      const selected = stmts.filter((statement) =>
        operationAllowed(statement.operation, syncOptions),
      );
      if (selected.length === 0) {
        setSyncState('compared');
        setExecuteProgress('');
        return;
      }
      setExecuteProgress(t('sync.executingSql', { count: selected.length }));
      writeStarted = true;
      writeInFlightRef.current = true;
      const selectedProfile = syncProfiles.find((profile) => profile.id === selectedProfileId);
      const profileRef = selectedProfile
        ? { id: selectedProfile.id, revision: selectedProfile.updatedAt }
        : undefined;
      const result = tableSelections.length
        ? profileRef
          ? await syncCommands.executeDataSync(
              tgtConnId,
              selected,
              jobId,
              targetDatabase,
              selectedRows,
              tableSelections,
              profileRef,
            )
          : await syncCommands.executeDataSync(
              tgtConnId,
              selected,
              jobId,
              targetDatabase,
              selectedRows,
              tableSelections,
            )
        : profileRef
          ? await syncCommands.executeDataSync(
              tgtConnId,
              selected,
              jobId,
              targetDatabase,
              undefined,
              undefined,
              profileRef,
            )
          : await syncCommands.executeDataSync(tgtConnId, selected, jobId, targetDatabase);
      writeStarted = false;
      executionResolved = true;
      writeInFlightRef.current = false;
      setLastExecutionResult(result);

      const executionOutcome = result.outcome ?? (result.rolledBack ? 'rolled_back' : 'committed');
      if (executionOutcome === 'not_started') {
        setErrorMsg(result.error || t('sync.executionNotStarted'));
        setErrorOpen(true);
        setWriteOutcomeUncertain(false);
        setSyncState('compared');
        setExecuteProgress('');
        return;
      }
      if (executionOutcome === 'unknown') {
        setErrorMsg(`${t('sync.executionUnknown')} ${result.error || ''}`.trim());
        setErrorOpen(true);
        setWriteOutcomeUncertain(true);
        setSyncState('unknown');
        setStep('result');
        setExecuteProgress('');
        return;
      }

      setExecuteProgress(t('sync.recomparing'));
      const recompareFilters = Object.fromEntries(
        tablesWithSelection
          .filter((row) => row.sourceFilter)
          .map((row) => [row.sourceTable, row.sourceFilter as DataSyncSourceFilter]),
      );
      const recomparedResponse = Object.keys(recompareFilters).length
        ? await syncCommands.compareDataSync(
            srcConnId,
            tgtConnId,
            tablesWithSelection.map((r) => r.sourceTable),
            jobId,
            sourceDatabase,
            targetDatabase,
            sourceSchema || undefined,
            targetSchema || undefined,
            syncOptions,
            recompareFilters,
          )
        : await syncCommands.compareDataSync(
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
      const recompared = Array.isArray(recomparedResponse)
        ? recomparedResponse
        : recomparedResponse.tables;
      const pagedRecompare =
        !Array.isArray(recomparedResponse) && recomparedResponse.contractVersion != null;
      if (pagedRecompare) {
        setSelectedRows([]);
        setTableSelections([]);
        setPageIndex({});
        setPageCursors({});
        loadedPageRef.current.clear();
      }
      setMappingResults((prev) => {
        const merged = mergeCompareIntoMappings(prev, recompared);
        return merged.map((row) => {
          if (pagedRecompare) return { ...row, rows: undefined };
          if (row.status !== 'MATCHED' || !row.rows) return row;
          return { ...row, rows: applyOptionsToRows(row.rows, syncOptions) };
        });
      });
      if (!pagedRecompare) {
        setSelectedRows(
          (recompared as DataSyncTableResult[]).flatMap((table) =>
            (table.rows ?? [])
              .filter((row) => row.selected && row.operation !== 'UNCHANGED')
              .map((row) => ({
                sourceTable: table.sourceTable,
                targetTable: table.targetTable,
                operation: row.operation,
                key: row.key,
              })),
          ),
        );
      }
      setWriteOutcomeUncertain(false);
      setSyncState('done');
      setStep('result');
      setExecuteProgress('');
      setStatusMsg('');
    } catch (e) {
      setErrorMsg(
        `${writeStarted ? t('sync.executionUnknown') + ' ' : ''}${e instanceof Error ? e.message : String(e)}`,
      );
      setErrorOpen(true);
      if (writeStarted) {
        setWriteOutcomeUncertain(true);
        setSyncState('unknown');
      } else if (executionResolved) {
        setWriteOutcomeUncertain(false);
        setSyncState('done');
        setStep('result');
      } else {
        setSyncState('compared');
      }
      setExecuteProgress('');
    } finally {
      writeInFlightRef.current = false;
      if (cancellingStatusJobRef.current === jobId) {
        cancellingStatusJobRef.current = null;
        setStatusMsg((current) => (current === t('sync.cancellingExecution') ? '' : current));
      }
      if (jobIdRef.current === jobId && jobKindRef.current === 'execute') {
        jobIdRef.current = null;
        jobKindRef.current = null;
      }
      if (cancelRequestedJobRef.current === jobId) cancelRequestedJobRef.current = null;
    }
  }, [
    sourceId,
    targetId,
    mappingResults,
    selectedRows,
    tableSelections,
    syncOptions,
    refreshEndpointSessions,
    sourceDatabase,
    targetDatabase,
    sourceSchema,
    targetSchema,
    targetReadOnly,
    t,
    writeOutcomeUncertain,
  ]);

  const handleExecute = useCallback(() => {
    if (targetReadOnly) {
      setErrorMsg(t('sync.targetReadOnly'));
      setErrorOpen(true);
      return;
    }
    if (hasSelectedDeletes) {
      setExecuteConfirmOpen(true);
      return;
    }
    void runExecute();
  }, [targetReadOnly, hasSelectedDeletes, runExecute]);

  const updateTableRows = useCallback(
    (key: string, rows: DataSyncRowChange[]) => {
      const table = mappingResults.find((row) => tableKey(row) === key);
      if (!table) return;
      setSelectedRows((previous) => {
        const pageTokens = new Set(
          rows
            .filter((row) => row.operation !== 'UNCHANGED')
            .map((row) =>
              selectedRowToken({
                sourceTable: table.sourceTable,
                targetTable: table.targetTable,
                operation: row.operation,
                key: row.key,
              }),
            ),
        );
        const retained = previous.filter((row) => {
          if (!pageTokens.has(selectedRowToken(row))) return true;
          const scope = scopeForOperation(tableSelectionsRef.current, table, row.operation);
          return Boolean(scope && scopeSelectsOperation(scope, row.operation, syncOptions));
        });
        const next = rows
          .filter((row) => {
            if (!row.selected || row.operation === 'UNCHANGED') return false;
            const scope = scopeForOperation(tableSelectionsRef.current, table, row.operation);
            return !(scope && scopeSelectsOperation(scope, row.operation, syncOptions));
          })
          .map((row) => ({
            sourceTable: table.sourceTable,
            targetTable: table.targetTable,
            operation: row.operation,
            key: row.key,
          }));
        const known = new Set(retained.map(selectedRowToken));
        return [...retained, ...next.filter((row) => !known.has(selectedRowToken(row)))];
      });
      setTableSelections((previous) =>
        previous.map((candidate) => {
          if (
            candidate.sourceTable !== table.sourceTable ||
            candidate.targetTable !== table.targetTable
          )
            return candidate;
          const candidateRows = rows.filter((row) =>
            candidate.operations.includes(
              row.operation as Exclude<DataSyncRowChange['operation'], 'UNCHANGED'>,
            ),
          );
          const exclusions = candidate.excludedRows.filter(
            (excluded) =>
              !candidateRows.some(
                (row) =>
                  row.operation === excluded.operation &&
                  rowKeyString(row.key) === rowKeyString(excluded.key),
              ),
          );
          const additions: DataSyncSelectionExclusion[] = candidateRows
            .filter(
              (row) =>
                scopeSelectsOperation(candidate, row.operation, syncOptions) && !row.selected,
            )
            .map((row) => ({
              operation: row.operation as Exclude<DataSyncRowChange['operation'], 'UNCHANGED'>,
              key: row.key,
            }))
            .filter(
              (row) =>
                !exclusions.some(
                  (excluded) =>
                    excluded.operation === row.operation &&
                    rowKeyString(excluded.key) === rowKeyString(row.key),
                ),
            );
          return { ...candidate, excludedRows: [...exclusions, ...additions] };
        }),
      );
      setMappingResults((prev) => prev.map((r) => (tableKey(r) === key ? { ...r, rows } : r)));
    },
    [mappingResults, syncOptions],
  );

  const setTableOperationScope = useCallback(
    (
      table: DataSyncTableResult,
      operation: Exclude<DataSyncRowChange['operation'], 'UNCHANGED'>,
    ) => {
      if (!operationAllowed(operation, syncOptions)) return;
      setTableSelections((previous) => {
        const allScope = previous.find(
          (scope) =>
            scope.sourceTable === table.sourceTable &&
            scope.targetTable === table.targetTable &&
            scope.selectionMode === 'all',
        );
        if (allScope?.operations.includes(operation)) return previous;
        const cleaned = previous
          .map((scope) =>
            scope.sourceTable === table.sourceTable && scope.targetTable === table.targetTable
              ? {
                  ...scope,
                  operations: scope.operations.filter((candidate) => candidate !== operation),
                  excludedRows: scope.excludedRows.filter((row) => row.operation !== operation),
                }
              : scope,
          )
          .filter((scope) => scope.operations.length > 0);
        if (allScope) {
          return cleaned.map((scope) =>
            scope.sourceTable === table.sourceTable &&
            scope.targetTable === table.targetTable &&
            scope.selectionMode === 'all'
              ? { ...scope, operations: [...scope.operations, operation] }
              : scope,
          );
        }
        return [
          ...cleaned,
          {
            sourceTable: table.sourceTable,
            targetTable: table.targetTable,
            selectionMode: 'all',
            operations: [operation],
            excludedRows: [],
          },
        ];
      });
      setSelectedRows((previous) =>
        previous.filter(
          (row) =>
            row.sourceTable !== table.sourceTable ||
            row.targetTable !== table.targetTable ||
            row.operation !== operation,
        ),
      );
      setMappingResults((previous) =>
        previous.map((candidate) =>
          tableKey(candidate) === tableKey(table)
            ? {
                ...candidate,
                rows: candidate.rows?.map((row) =>
                  row.operation === operation ? { ...row, selected: true } : row,
                ),
              }
            : candidate,
        ),
      );
    },
    [setMappingResults, syncOptions],
  );

  const clearTableOperationScope = useCallback(
    (
      table: DataSyncTableResult,
      operation: Exclude<DataSyncRowChange['operation'], 'UNCHANGED'>,
    ) => {
      setTableSelections((previous) => {
        const cleaned = previous
          .map((scope) =>
            scope.sourceTable === table.sourceTable && scope.targetTable === table.targetTable
              ? {
                  ...scope,
                  operations: scope.operations.filter((candidate) => candidate !== operation),
                  excludedRows: scope.excludedRows.filter((row) => row.operation !== operation),
                }
              : scope,
          )
          .filter((scope) => scope.operations.length > 0);
        const defaultsScope = cleaned.find(
          (scope) =>
            scope.sourceTable === table.sourceTable &&
            scope.targetTable === table.targetTable &&
            scope.selectionMode === 'defaults',
        );
        if (defaultsScope) {
          return cleaned.map((scope) =>
            scope === defaultsScope
              ? { ...scope, operations: [...scope.operations, operation] }
              : scope,
          );
        }
        return [
          ...cleaned,
          {
            sourceTable: table.sourceTable,
            targetTable: table.targetTable,
            selectionMode: 'defaults',
            operations: [operation],
            excludedRows: [],
          },
        ];
      });
      setSelectedRows((previous) =>
        previous.filter(
          (row) =>
            row.sourceTable !== table.sourceTable ||
            row.targetTable !== table.targetTable ||
            row.operation !== operation,
        ),
      );
      setMappingResults((previous) =>
        previous.map((candidate) =>
          tableKey(candidate) === tableKey(table)
            ? {
                ...candidate,
                rows: candidate.rows?.map((row) =>
                  row.operation === operation
                    ? { ...row, selected: defaultRowSelected(row.operation, syncOptions) }
                    : row,
                ),
              }
            : candidate,
        ),
      );
    },
    [setMappingResults, syncOptions],
  );

  const isTableOperationScoped = useCallback(
    (table: DataSyncTableResult, operation: Exclude<DataSyncRowChange['operation'], 'UNCHANGED'>) =>
      tableSelections.some(
        (scope) =>
          scope.sourceTable === table.sourceTable &&
          scope.targetTable === table.targetTable &&
          scope.operations.includes(operation),
      ),
    [tableSelections],
  );

  const busy = syncState === 'inspecting' || syncState === 'comparing' || syncState === 'executing';
  const compareDisabled = Boolean(sourceSessionError || targetSessionError);
  const stepIndex = WIZARD_STEPS.indexOf(step);
  const canNext = useMemo(() => {
    switch (step) {
      case 'endpoints':
        return Boolean(
          sourceId &&
            targetId &&
            sourceDatabase &&
            targetDatabase &&
            activePairing?.supported &&
            !scopeReconfirmationRequired &&
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
    scopeReconfirmationRequired,
    compareDisabled,
    busy,
    inspectionComplete,
    mappingResults,
    compared,
  ]);

  const goNext = useCallback(async () => {
    const next = WIZARD_STEPS[stepIndex + 1];
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
    const previous = WIZARD_STEPS[stepIndex - 1];
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

  const lastExecutionOutcome =
    lastExecutionResult?.outcome ?? (lastExecutionResult?.rolledBack ? 'rolled_back' : 'committed');
  const lastExecutionIsUnknown = lastExecutionOutcome === 'unknown';
  const lastExecutionWasRolledBack = lastExecutionOutcome === 'rolled_back';

  // All hooks above. Gate the body on the `sync` locale pack so the UI never
  // renders raw/un-translated `t('sync.*')` keys before it is imported.
  if (!localesReady) {
    return <LocaleDomainLoading testId="data-sync-locale-loading" />;
  }

  return (
    <div
      data-testid="data-sync-window"
      data-sync-state={syncState}
      data-sync-step={step}
      data-write-outcome-uncertain={writeOutcomeUncertain ? 'true' : 'false'}
      className="flex h-screen min-h-0 flex-col bg-surface text-fg"
    >
      <TitleBar
        title={t('common.dataSyncTitle')}
        rightContent={
          <MigrationRunHistoryDialog operation="dataSync" onReconcile={reconcileUnknownRun} />
        }
      />

      <div className="border-b border-edge px-6 py-3">
        <div className="mx-auto flex max-w-4xl flex-wrap items-center justify-center gap-1">
          {WIZARD_STEPS.map((s, i) => (
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
                      ? 'bg-accent text-on-accent'
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
            NARROW_WIZARD_STEPS.includes(step) ? 'max-w-2xl' : 'max-w-6xl',
          )}
        >
          {step === 'endpoints' && (
            <div className="space-y-4">
              {scopeReconfirmationRequired && (
                <div
                  className="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-warning/40 bg-warning/10 p-3"
                  data-testid="data-sync-recovery-scope-review"
                  role="status"
                >
                  <p className="text-xs text-fg-secondary">
                    {t('migrationHistory.syncReconcileNeedsScope')}
                  </p>
                  <Button
                    variant="secondary"
                    size="sm"
                    data-testid="data-sync-reconfirm-scope"
                    onClick={() => setScopeReconfirmationRequired(false)}
                  >
                    {t('migrationHistory.syncReconcileConfirmScope')}
                  </Button>
                </div>
              )}
              <div
                data-testid="data-sync-profile-controls"
                className="flex flex-wrap items-end gap-2 rounded-lg border border-edge bg-surface-alt p-3"
              >
                <label className="min-w-48 flex-1 text-xs text-fg-muted">
                  <span className="mb-1 block">{t('sync.profile.name')}</span>
                  <Input
                    data-testid="data-sync-profile-name"
                    value={profileName}
                    onChange={(event) => setProfileName(event.target.value)}
                    placeholder={t('sync.profile.namePlaceholder')}
                    className="h-8 w-full px-2 text-sm"
                  />
                </label>
                <label className="min-w-48 text-xs text-fg-muted">
                  <span className="mb-1 block">{t('sync.profile.load')}</span>
                  <Select
                    className="w-full"
                    triggerDataAttrs={{ 'data-testid': 'data-sync-profile-select' }}
                    value={selectedProfileId}
                    placeholder={t('sync.profile.select')}
                    options={syncProfiles.map((profile) => ({
                      value: profile.id,
                      label: profile.name,
                    }))}
                    onChange={setSelectedProfileId}
                  />
                </label>
                <Button
                  size="sm"
                  data-testid="data-sync-profile-save"
                  onClick={() => void saveCurrentProfile()}
                >
                  {t('sync.profile.save')}
                </Button>
                <Button
                  variant="secondary"
                  size="sm"
                  data-testid="data-sync-profile-load"
                  disabled={!selectedProfileId}
                  onClick={loadSelectedProfile}
                >
                  {t('sync.profile.load')}
                </Button>
                <Button
                  variant="ghost"
                  size="sm"
                  data-testid="data-sync-profile-delete"
                  disabled={!selectedProfileId}
                  onClick={() => void deleteSelectedProfile()}
                >
                  {t('sync.profile.delete')}
                </Button>
              </div>
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
                experimental={Boolean(
                  sourceConn &&
                    targetConn &&
                    activePairing?.supported &&
                    !isVerifiedMigrationPair(sourceConn.databaseType, targetConn.databaseType),
                )}
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
            </div>
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
                  <Spinner size="xl" tone="accent" />
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
                  onUpdateSourceFilter={updateSourceFilter}
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
                  <Spinner size="lg" />
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
                          onSelectAllOperation={(operation) =>
                            setTableOperationScope(selectedTable, operation)
                          }
                          onClearAllOperation={(operation) =>
                            clearTableOperationScope(selectedTable, operation)
                          }
                          isOperationSelected={(operation) =>
                            isTableOperationScoped(selectedTable, operation)
                          }
                          hasTableSelection={tableSelections.some(
                            (scope) =>
                              scope.sourceTable === selectedTable.sourceTable &&
                              scope.targetTable === selectedTable.targetTable,
                          )}
                          pageLoading={pageLoading}
                          pageIndex={
                            pageIndex[
                              `${selectedTable.sourceTable}\u0000${selectedTable.targetTable}`
                            ] ?? 0
                          }
                          hasPreviousPage={Boolean(
                            pageCursors[
                              `${selectedTable.sourceTable}\u0000${selectedTable.targetTable}`
                            ]?.previous.length,
                          )}
                          hasNextPage={Boolean(
                            pageCursors[
                              `${selectedTable.sourceTable}\u0000${selectedTable.targetTable}`
                            ]?.next,
                          )}
                          onPageChange={(direction) => void loadTablePage(selectedTable, direction)}
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
            <div className="relative flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border border-edge">
              {syncState === 'executing' && (
                <div
                  data-testid="data-sync-executing-overlay"
                  className="absolute inset-0 z-10 flex flex-col items-center justify-center gap-2 bg-surface/90 text-sm text-fg-muted"
                >
                  <Spinner size="2xl" tone="accent" />
                  <span>{executeProgress || t('sync.executing')}</span>
                </div>
              )}
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
                  selectedRows={selectedRows}
                  tableSelections={tableSelections}
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
                className={cn(
                  'flex flex-wrap items-center gap-3 text-sm',
                  lastExecutionIsUnknown || lastExecutionWasRolledBack
                    ? 'text-amber-700 dark:text-amber-400'
                    : 'text-green-700 dark:text-green-400',
                )}
                role="status"
              >
                <span>
                  {lastExecutionIsUnknown
                    ? t('sync.executionUnknown')
                    : lastExecutionWasRolledBack
                      ? lastExecutionResult?.rollbackReason || t('sync.rolledBack')
                      : t('sync.executeDone')}
                </span>
                {lastExecutionResult?.skipped ? (
                  <span className="text-amber-600 dark:text-amber-400">
                    {t('sync.conflictsSkipped', { count: lastExecutionResult.skipped })}
                  </span>
                ) : null}
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
          canExecute={
            !writeOutcomeUncertain &&
            mappingResults.some((r) => r.status === 'MATCHED' && tableHasRowDiffs(r))
          }
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
          <ChevronLeft className="h-4 w-4" /> {t('sync.back')}
        </Button>
        {step !== 'preview' && step !== 'result' ? (
          <Button
            data-testid="data-sync-next"
            disabled={!canNext || busy}
            onClick={() => void goNext()}
          >
            {busy ? <Spinner size="lg" /> : null}
            {t('sync.next')}
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
        <CopyableError
          message={errorMsg}
          className="error-message text-sm"
          data-testid="data-sync-error"
        />
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
            <Spinner size="lg" />
            {t('sync.executing')}
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
