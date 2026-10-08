import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { ChevronLeft, ChevronRight } from 'lucide-react';
import { TitleBar } from '../../components/TitleBar';
import { StatusBar } from '../../components/StatusBar';
import { LocaleDomainLoading } from '../../components/LocaleDomainLoading';
import { Button } from '../../components/ui/Button';
import { Checkbox } from '../../components/ui/Checkbox';
import { Dialog } from '../../components/ui/Dialog';
import { CopyableError } from '../../components/ui/CopyableError';
import { ErrorBanner } from '../../components/ui/ErrorBanner';
import { Radio } from '../../components/ui/Radio';
import { Select } from '../../components/ui/Select';
import { Input } from '../../components/ui/Input';
import {
  DEFAULT_TRANSFER_OPTIONS,
  transferCommands,
  type TransferJob,
  type TransferMode,
  type TransferPreview,
  type TransferProfile,
  type TransferTableMapping,
  type TransferTableResult,
  type TransferSqlFileTarget,
  type WriteMode,
} from '../../commands/transfer';
import { useI18n } from '../../hooks/useI18n';
import { useLocaleDomains } from '../../hooks/useLocaleDomains';
import { useSettings } from '../../hooks/useSettings';
import { useSettingsStore } from '../../stores/settingsStore';
import { connectionCommands } from '../../commands/connection';
import { cn } from '../../lib/cn';
import { DB_REGISTRY } from '../../lib/databaseTypes';
import { listenCrossWindow } from '../../lib/crossWindowBus';
import {
  isTransferLimitationsDismissed,
  setTransferLimitationsDismissed,
} from '../../lib/transferLimitationsPrefs';
import { isTransferTargetSupported, resolveTransferPairing } from '../../lib/transferPairing';
import { isVerifiedMigrationPair } from '../../lib/migrationVerification';
import type { ConnectionConfig } from '../../types';
import { LimitationsDialog } from '../../components/ui/LimitationsDialog';
import { Spinner } from '../../components/ui/Spinner';
import { TRANSFER_LIMITATION_KEYS } from './transferLimitationKeys';
import { TransferExecuteConfirmDialog } from './TransferExecuteConfirmDialog';
import { TransferMappingStep } from './TransferMappingStep';
import {
  MigrationEndpointsBar,
  TransferPairingNote,
} from '../../components/migration/MigrationEndpointsBar';
import { MigrationRunHistoryDialog } from '../../components/migration/MigrationRunHistoryDialog';
import {
  normalizeColumnMappings,
  mappingGateAllowsAdvance,
} from './transferMappingView';
import { MappingGateNotice } from './MappingGateNotice';
import { SqlCodeBlock } from '../../components/SqlCodeBlock';
import {
  ensureDedicatedSession,
  listDatabasesDedicated,
  releaseDedicatedSession,
  type DedicatedSideSession,
} from '../../lib/dedicatedDbSession';
import {
  pickPrefillDatabase,
  resolveDefaultDatabase,
  useMigrationEndpointPrefill,
} from '../../lib/migrationWindowPrefill';
import { useMigrationJobHydration } from '../../hooks/useMigrationJobHydration';
import { useTransferJobRun } from '../../hooks/useTransferJobRun';
import { TransferJobResultPanel } from './TransferJobResultPanel';
import { MigrationJobFailureNotice } from '../../components/migration/MigrationJobFailureNotice';

type WizardStep = 'endpoints' | 'setup' | 'objects' | 'mapping' | 'preview' | 'result';

const STEPS: WizardStep[] = ['endpoints', 'setup', 'objects', 'mapping', 'preview', 'result'];

const NARROW_STEPS: WizardStep[] = ['endpoints', 'setup', 'result'];

export function DataTransferWindow() {
  const localesReady = useLocaleDomains(['sync']);
  useSettings();
  const { t } = useI18n();
  const loadSettings = useSettingsStore((s) => s.loadSettings);

  const [connections, setConnections] = useState<ConnectionConfig[]>([]);
  const migrationJobs = useMigrationJobHydration('dataTransfer');
  /**
   * P5 Job path. Every run now goes `prepare` → `apply`; the window keeps only
   * the review payload (`preview`) in its own state because the rest of the
   * wizard is written against `TransferPreview`.
   */
  const jobRun = useTransferJobRun();
  // The callbacks are `useCallback`-stable; `jobRun` itself is a fresh object
  // every render, so the wizard's `useCallback` blocks depend on these instead
  // or they would be rebuilt (and re-trigger their effects) on every render.
  const {
    prepare: prepareJob,
    apply: applyJob,
    requestCancel: requestJobCancel,
    reset: resetJobRun,
    lastFailure: lastJobFailure,
  } = jobRun;
  const [sourceSession, setSourceSession] = useState<DedicatedSideSession | null>(null);
  const [targetSession, setTargetSession] = useState<DedicatedSideSession | null>(null);
  const [sourceId, setSourceId] = useState('');
  const [targetId, setTargetId] = useState('');
  const [sourceDatabases, setSourceDatabases] = useState<string[]>([]);
  const [targetDatabases, setTargetDatabases] = useState<string[]>([]);
  const [sourceDatabase, setSourceDatabase] = useState('');
  const [targetDatabase, setTargetDatabase] = useState('');
  const [destinationMode, setDestinationMode] = useState<'database' | 'sqlFile'>('database');
  const [sqlFileTarget, setSqlFileTarget] = useState<TransferSqlFileTarget | null>(null);
  const [sqlFileDialect, setSqlFileDialect] = useState('source');
  const [sqlFileEncoding, setSqlFileEncoding] = useState<
    'utf8' | 'utf8Bom' | 'utf16Le' | 'utf16Be'
  >('utf8');
  const [sqlFileCompression, setSqlFileCompression] = useState<'none' | 'gzip'>('none');
  const [sqlFileDatabase, setSqlFileDatabase] = useState('');
  const [sqlFileSchema, setSqlFileSchema] = useState('');
  const [availableSqlDialects, setAvailableSqlDialects] = useState<string[]>([]);
  const [mode, setMode] = useState<TransferMode>('data');
  const [writeMode, setWriteMode] = useState<WriteMode>('insert');
  const [tables, setTables] = useState<TransferTableResult[]>([]);
  const [preview, setPreview] = useState<TransferPreview | null>(null);
  const [step, setStep] = useState<WizardStep>('endpoints');
  const [batchSize, setBatchSize] = useState(DEFAULT_TRANSFER_OPTIONS.batchSize ?? 500);
  const [stopOnError, setStopOnError] = useState(true);
  const [confirmedDestructive, setConfirmedDestructive] = useState(false);
  const [useTargetDefaultCollation, setUseTargetDefaultCollation] = useState(false);
  const [loading, setLoading] = useState(false);
  /**
   * True for exactly the window between "the user asked to prepare a
   * plan" and "the prepare resolved". It is set *before* the await, so the
   * mapping editor is already inert when the round trip starts — there is no
   * scheduling gap in which the editor could rewrite the very plan being
   * prepared.
   */
  const [preparing, setPreparing] = useState(false);
  const [executing, setExecuting] = useState(false);
  const [executeProgress, setExecuteProgress] = useState('');
  const [errorMsg, setErrorMsg] = useState('');
  const [errorOpen, setErrorOpen] = useState(false);
  const [previewError, setPreviewError] = useState('');
  /** Set only by the post-prepare re-check on the mapping step. */
  const [mappingGateError, setMappingGateError] = useState('');
  const [limitationsOpen, setLimitationsOpen] = useState(false);
  const [executeConfirmOpen, setExecuteConfirmOpen] = useState(false);
  const [selectedMappingTable, setSelectedMappingTable] = useState('');
  const [transferProfiles, setTransferProfiles] = useState<TransferProfile[]>([]);
  const [selectedProfileId, setSelectedProfileId] = useState('');
  const [profileName, setProfileName] = useState('');
  const profileMappingsRef = useRef<TransferTableMapping[] | null>(null);
  const prepareSeqRef = useRef(0);
  /**
   * The rows as they are *now*, not as they were when the user clicked
   * Next. Synced by effect (never assigned during render) so the post-await
   * re-check reads whatever landed while prepare was in flight.
   */
  const tablesRef = useRef<TransferTableResult[]>(tables);
  useEffect(() => {
    tablesRef.current = tables;
  }, [tables]);

  /**
   * The backend keys its receipt map by `idempotencyKey` and mints a fresh
   * one when the client sends none — which would make every retry of a lost
   * commit receipt a brand-new write attempt against a plan that may already
   * have committed. So the apply key is derived from the admitted `planId`: a
   * retry repeats it and the receipt map can answer, while the prepare key is
   * per call, because a *new* preview is a new admission and must never be
   * answered with an older prepare Job's receipt.
   */
  const nextPrepareKey = useCallback(() => {
    prepareSeqRef.current += 1;
    return `data-transfer/prepare/${prepareSeqRef.current}`;
  }, []);
  const applyKeyForPlan = useCallback(
    (planId: string) => `data-transfer/apply/${planId}`,
    [],
  );

  useEffect(() => {
    void loadSettings();
  }, [loadSettings]);

  const loadTransferProfiles = useCallback(() => {
    void transferCommands
      .getProfiles()
      .then(setTransferProfiles)
      .catch(() => setTransferProfiles([]));
  }, []);

  useEffect(() => {
    loadTransferProfiles();
  }, [loadTransferProfiles]);

  useEffect(() => {
    if (!isTransferLimitationsDismissed()) {
      setLimitationsOpen(true);
    }
  }, []);

  const loadConnections = useCallback(() => {
    void invoke<ConnectionConfig[]>('get_connections')
      .then(setConnections)
      .catch((e) => console.error('Failed to load connections', e));
  }, []);

  useEffect(() => {
    loadConnections();
  }, [loadConnections]);

  useEffect(() => {
    void connectionCommands
      .getAvailableDrivers()
      .then((drivers) => {
        if (!Array.isArray(drivers)) return;
        setAvailableSqlDialects(
          drivers.filter((driver) => {
            const meta = DB_REGISTRY[driver as keyof typeof DB_REGISTRY];
            return meta?.supportsSQL === true && meta.category === 'sql';
          }),
        );
      })
      .catch(() => setAvailableSqlDialects([]));
  }, []);

  const migrationPrefillRef = useMigrationEndpointPrefill(connections, setSourceId, setTargetId);

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
    const experimentalHint = t('common.experimentalPairHint');
    const srcType = sourceConn?.databaseType;
    return connections.map((c) => {
      const unsupported = Boolean(srcType && !isTransferTargetSupported(srcType, c.databaseType));
      const base = `${c.name} (${c.databaseType})`;
      const experimental = Boolean(
        srcType && !unsupported && !isVerifiedMigrationPair(srcType, c.databaseType),
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
  }, [connections, sourceConn?.databaseType, t]);

  const pairing = useMemo(() => {
    if (!sourceConn || !targetConn) return null;
    return resolveTransferPairing(sourceConn.databaseType, targetConn.databaseType);
  }, [sourceConn, targetConn]);

  const targetReadOnly = targetConn?.readOnly === true;

  const chooseSqlFile = useCallback(async () => {
    try {
      const picked = await transferCommands.pickSqlFile();
      if (picked) {
        setSqlFileTarget(picked);
        setDestinationMode('sqlFile');
      }
    } catch (e) {
      setErrorMsg(e instanceof Error ? e.message : String(e));
      setErrorOpen(true);
    }
  }, []);

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

  useEffect(() => {
    if (!sourceId) {
      setSourceDatabases([]);
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
      } catch {
        if (!cancelled) setSourceDatabases([]);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [sourceId, connections]);

  useEffect(() => {
    if (!sourceId || !sourceDatabase) {
      setSourceSession(null);
      return;
    }
    let cancelled = false;
    (async () => {
      const next = await ensureDedicatedSession(sourceSession, sourceId, sourceDatabase);
      if (!cancelled) setSourceSession(next);
    })();
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- reconnect when endpoint or catalog changes
  }, [sourceId, sourceDatabase]);

  useEffect(() => {
    if (!targetId) {
      setTargetDatabases([]);
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
      } catch {
        if (!cancelled) setTargetDatabases([]);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [targetId, connections]);

  useEffect(() => {
    if (!targetId || !targetDatabase) {
      setTargetSession(null);
      return;
    }
    let cancelled = false;
    (async () => {
      const next = await ensureDedicatedSession(targetSession, targetId, targetDatabase);
      if (!cancelled) setTargetSession(next);
    })();
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- reconnect when endpoint or catalog changes
  }, [targetId, targetDatabase]);

  const allTablesToMappings = useCallback(
    (): TransferTableMapping[] =>
      tables
        .filter((tbl) => tbl.sourceTable)
        .map((tbl) => ({
          sourceTable: tbl.sourceTable,
          targetTable: tbl.targetTable,
          createNew: tbl.createNew,
          enabled: tbl.enabled,
          columnMappings: normalizeColumnMappings(tbl),
          ddlOverride: tbl.ddlOverride?.trim() ? tbl.ddlOverride.trim() : undefined,
          sourceFilter: tbl.sourceFilter,
          recordset: tbl.recordset,
        })),
    [tables],
  );

  const tablesToMappings = useCallback(
    (): TransferTableMapping[] => allTablesToMappings().filter((table) => table.enabled !== false),
    [allTablesToMappings],
  );

  const saveCurrentProfile = useCallback(async () => {
    const name = profileName.trim();
    if (!name || !sourceId || !sourceDatabase) {
      setErrorMsg(t('transfer.profile.missingFields'));
      setErrorOpen(true);
      return;
    }
    if (destinationMode === 'database' && (!targetId || !targetDatabase)) {
      setErrorMsg(t('transfer.profile.missingFields'));
      setErrorOpen(true);
      return;
    }
    const now = new Date().toISOString();
    const existing = transferProfiles.find((profile) => profile.id === selectedProfileId);
    const profile: TransferProfile = {
      version: 1,
      id: existing?.id ?? crypto.randomUUID(),
      name,
      sourceConnectionId: sourceId,
      targetConnectionId: destinationMode === 'database' ? targetId : null,
      sourceDatabase,
      targetDatabase: destinationMode === 'database' ? targetDatabase : null,
      sourceSchema: sourceConn?.schema ?? null,
      targetSchema: destinationMode === 'database' ? (targetConn?.schema ?? null) : null,
      destinationMode,
      sqlFileDialect: destinationMode === 'sqlFile' ? sqlFileDialect : null,
      sqlFileEncoding: destinationMode === 'sqlFile' ? sqlFileEncoding : null,
      sqlFileCompression: destinationMode === 'sqlFile' ? sqlFileCompression : null,
      sqlFileDatabase: destinationMode === 'sqlFile' ? sqlFileDatabase || null : null,
      sqlFileSchema: destinationMode === 'sqlFile' ? sqlFileSchema || null : null,
      mode,
      writeMode,
      tables: allTablesToMappings(),
      options: { batchSize, stopOnError, confirmedDestructive, useTargetDefaultCollation },
      createdAt: existing?.createdAt ?? now,
      updatedAt: now,
    };
    try {
      await transferCommands.saveProfile(profile);
      setSelectedProfileId(profile.id);
      setProfileName(profile.name);
      loadTransferProfiles();
    } catch (error) {
      setErrorMsg(error instanceof Error ? error.message : String(error));
      setErrorOpen(true);
    }
  }, [
    profileName,
    sourceId,
    sourceDatabase,
    destinationMode,
    targetId,
    targetDatabase,
    transferProfiles,
    selectedProfileId,
    sourceConn?.schema,
    targetConn?.schema,
    sqlFileDialect,
    sqlFileEncoding,
    sqlFileCompression,
    sqlFileDatabase,
    sqlFileSchema,
    mode,
    writeMode,
    allTablesToMappings,
    batchSize,
    stopOnError,
    confirmedDestructive,
    useTargetDefaultCollation,
    loadTransferProfiles,
    t,
  ]);

  const loadSelectedProfile = useCallback(() => {
    const profile = transferProfiles.find((candidate) => candidate.id === selectedProfileId);
    if (!profile) return;
    setProfileName(profile.name);
    setSourceId(profile.sourceConnectionId);
    setTargetId(profile.targetConnectionId ?? '');
    setSourceDatabase(profile.sourceDatabase ?? '');
    setTargetDatabase(profile.targetDatabase ?? '');
    setMode(profile.mode);
    setWriteMode(profile.writeMode);
    setBatchSize(profile.options.batchSize ?? DEFAULT_TRANSFER_OPTIONS.batchSize ?? 500);
    setStopOnError(profile.options.stopOnError ?? true);
    setConfirmedDestructive(profile.options.confirmedDestructive ?? false);
    setUseTargetDefaultCollation(profile.options.useTargetDefaultCollation ?? false);
    setDestinationMode(profile.destinationMode);
    setSqlFileDialect(profile.sqlFileDialect ?? 'source');
    setSqlFileEncoding(profile.sqlFileEncoding ?? 'utf8');
    setSqlFileCompression(profile.sqlFileCompression ?? 'none');
    setSqlFileDatabase(profile.sqlFileDatabase ?? '');
    setSqlFileSchema(profile.sqlFileSchema ?? '');
    setSqlFileTarget(null);
    profileMappingsRef.current = profile.tables;
    setTables([]);
    setPreview(null);
    // A different profile is a different plan: any run state from the previous
    // one must go with it, or a spent planId would look reusable.
    resetJobRun();
    setStep('endpoints');
    if (profile.destinationMode === 'sqlFile') {
      setErrorMsg(t('transfer.profile.chooseFile'));
      setErrorOpen(true);
    }
  }, [transferProfiles, selectedProfileId, t]);

  const updateTableDdlOverride = useCallback((sourceTable: string, ddl: string) => {
    setTables((prev) =>
      prev.map((row) => (row.sourceTable === sourceTable ? { ...row, ddlOverride: ddl } : row)),
    );
  }, []);

  const refreshEndpointSessions = useCallback(async () => {
    if (
      !sourceId ||
      !sourceDatabase ||
      (destinationMode === 'database' && (!targetId || !targetDatabase))
    ) {
      return {
        source: null as DedicatedSideSession | null,
        target: null as DedicatedSideSession | null,
      };
    }
    const source = await ensureDedicatedSession(sourceSession, sourceId, sourceDatabase);
    const target =
      destinationMode === 'database'
        ? await ensureDedicatedSession(targetSession, targetId, targetDatabase)
        : null;
    setSourceSession(source);
    setTargetSession(target);
    return { source, target };
  }, [
    sourceSession,
    targetSession,
    sourceId,
    targetId,
    sourceDatabase,
    targetDatabase,
    destinationMode,
  ]);

  const buildJob = useCallback(
    (sessions?: {
      source: DedicatedSideSession | null;
      target: DedicatedSideSession | null;
    }): TransferJob | null => {
      const srcConnId = sessions?.source?.dbSessionId ?? sourceSession?.dbSessionId;
      const tgtConnId = sessions?.target?.dbSessionId ?? targetSession?.dbSessionId;
      if (!srcConnId || !sourceDatabase) return null;
      if (destinationMode === 'sqlFile') {
        if (!sqlFileTarget) return null;
        return {
          source: { dbSessionId: srcConnId, database: sourceDatabase },
          sqlFileTarget: {
            ...sqlFileTarget,
            databaseType: sqlFileDialect === 'source' ? undefined : sqlFileDialect,
            encoding: sqlFileEncoding === 'utf8' ? undefined : sqlFileEncoding,
            compression: sqlFileCompression === 'none' ? undefined : sqlFileCompression,
            ...(sqlFileDatabase.trim() ? { database: sqlFileDatabase.trim() } : {}),
            ...(sqlFileSchema.trim() ? { schema: sqlFileSchema.trim() } : {}),
          },
          mode,
          writeMode,
          tables: tablesToMappings(),
          options: { batchSize, stopOnError, confirmedDestructive, useTargetDefaultCollation },
        };
      }
      if (!tgtConnId || !targetDatabase) return null;
      return {
        source: { dbSessionId: srcConnId, database: sourceDatabase },
        target: { dbSessionId: tgtConnId, database: targetDatabase },
        mode,
        writeMode,
        tables: tablesToMappings(),
        options: {
          batchSize,
          stopOnError,
          confirmedDestructive,
          useTargetDefaultCollation,
        },
      };
    },
    [
      sourceSession?.dbSessionId,
      targetSession?.dbSessionId,
      sourceDatabase,
      targetDatabase,
      destinationMode,
      sqlFileTarget,
      sqlFileDialect,
      sqlFileEncoding,
      sqlFileCompression,
      sqlFileDatabase,
      sqlFileSchema,
      mode,
      writeMode,
      tables,
      tablesToMappings,
      batchSize,
      stopOnError,
      confirmedDestructive,
      useTargetDefaultCollation,
    ],
  );

  const runInspect = useCallback(async () => {
    const { source, target } = await refreshEndpointSessions();
    const srcConnId = source?.dbSessionId;
    const tgtConnId = target?.dbSessionId;
    if (
      !srcConnId ||
      !sourceDatabase ||
      (destinationMode === 'database' && (!tgtConnId || !targetDatabase))
    ) {
      setErrorMsg(t('transfer.selectBoth'));
      setErrorOpen(true);
      return;
    }
    setLoading(true);
    try {
      const savedMappings = profileMappingsRef.current ?? [];
      const rows =
        destinationMode === 'sqlFile'
          ? await transferCommands.inspectSqlFile(
              srcConnId,
              mode,
              sourceDatabase,
              sourceConn?.schema,
              sqlFileDialect === 'source' ? undefined : sqlFileDialect,
              savedMappings,
            )
          : await transferCommands.inspect(
              srcConnId,
              tgtConnId!,
              mode,
              sourceDatabase,
              targetDatabase,
              savedMappings,
            );
      const enabled = rows
        .filter((r) => r.sourceTable)
        .map((r) => ({
          ...r,
          columnMappings: normalizeColumnMappings(r),
        }));
      setTables(enabled);
      if (enabled.length > 0) {
        setSelectedMappingTable((prev) =>
          prev && enabled.some((row) => row.sourceTable === prev)
            ? prev
            : (enabled.find((row) => row.enabled)?.sourceTable ?? enabled[0].sourceTable),
        );
      }
      profileMappingsRef.current = null;
    } catch (e) {
      setErrorMsg(e instanceof Error ? e.message : String(e));
      setErrorOpen(true);
    } finally {
      setLoading(false);
    }
  }, [
    refreshEndpointSessions,
    sourceDatabase,
    targetDatabase,
    sourceConn?.schema,
    sqlFileDialect,
    destinationMode,
    mode,
    t,
  ]);

  const runPreview = useCallback(
    async (opts?: { quiet?: boolean }): Promise<boolean> => {
      const sessions = await refreshEndpointSessions();
      const job = buildJob(sessions);
      if (!job) {
        setErrorMsg(t('transfer.selectBoth'));
        setErrorOpen(true);
        return false;
      }
      setPreviewError('');
      setMappingGateError('');
      setLoading(true);
      try {
        // The plan is admitted by `prepare_data_transfer_job`, which
        // runs its backend-scope check before anything else and returns the
        // same `TransferPreview` review the old `preview_data_transfer` did —
        // so every downstream consumer of `preview` is unchanged. The planId
        // it carries is the one `apply` must spend, and only once.
        const view = await prepareJob(job, { idempotencyKey: nextPrepareKey() });
        if (!view) {
          // The hook classified the refusal and re-renders with `failure`;
          // the wizard must not advance past a plan the backend declined to
          // admit, so this stays a non-advance. The backend's own words are
          // read synchronously — `prepare` resolves rather than throws, so
          // `failure` state would not be committed yet and the error panel
          // would fall back to a generic hint and lose the real cause.
          const detail = lastJobFailure()?.message;
          if (detail) setPreviewError(detail);
          return false;
        }
        setPreview(view.review);
        return true;
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        setPreview(null);
        setPreviewError(msg);
        if (!opts?.quiet) {
          setErrorMsg(msg);
          setErrorOpen(true);
        }
        return false;
      } finally {
        setLoading(false);
      }
    },
    [
      refreshEndpointSessions,
      buildJob,
      prepareJob,
      lastJobFailure,
      nextPrepareKey,
      t,
    ],
  );

  const runExecute = useCallback(
    async () => {
      const sessions = await refreshEndpointSessions();
      const job = buildJob(sessions);
      if (!job) return;
      if (destinationMode === 'database' && targetReadOnly) {
        setErrorMsg(t('transfer.readOnlyBlock'));
        setErrorOpen(true);
        return;
      }
      // The plan must come from `prepare`. Reading it off `preview` alone would
      // let the window apply a plan the hook never saw admitted, which is the
      // "spend it exactly once" rule stated backwards.
      const admitted = jobRun.prepareView;
      if (!admitted || !preview) {
        setErrorMsg(t('migrationJob.reprepareOnStalePlan'));
        setErrorOpen(true);
        setPreview(null);
        setStep('mapping');
        return;
      }
      setExecuting(true);
      const tableCount =
        tables.length > 0
          ? job.tables.filter((tbl) => tbl.enabled).length
          : Math.max(preview?.writePlans.length ?? 0, preview?.ddl.length ?? 0);
      setExecuteProgress(t('transfer.executingProgress', { count: tableCount }));
      // An absent selection means "every table the plan froze", which is
      // exactly the SQL-file case where the UI never inspected a target. An
      // explicit empty list would instead disable every table in the plan.
      const sourceTables =
        destinationMode === 'sqlFile' && tables.length === 0
          ? undefined
          : job.tables.filter((table) => table.enabled).map((table) => table.sourceTable);
      try {
        const view = await applyJob({
          sourceTables,
          confirmedDestructive,
          idempotencyKey: applyKeyForPlan(admitted.planId),
        });
        // A refusal (scope, budget, consumed plan) leaves `view` null and the
        // hook renders the fail-closed notice. Falling through to the result
        // step here would present a refusal as if a run had settled.
        setStep(view ? 'result' : 'preview');
      } finally {
        setExecuting(false);
        setExecuteProgress('');
      }
    },
    [
      refreshEndpointSessions,
      buildJob,
      jobRun.prepareView,
      applyJob,
      applyKeyForPlan,
      preview,
      targetReadOnly,
      destinationMode,
      tables.length,
      confirmedDestructive,
      t,
    ],
  );

  const handleExecuteClick = useCallback(() => {
    if (writeMode !== 'insert') {
      setExecuteConfirmOpen(true);
      return;
    }
    void runExecute();
  }, [writeMode, runExecute]);

  const handleExecuteConfirm = useCallback(() => {
    setExecuteConfirmOpen(false);
    void runExecute();
  }, [runExecute]);

  const handleCancel = useCallback(async () => {
    // A click is a *request*, not a cancellation. The hook decides which
    // Job id the request may legally be addressed to and records the
    // disposition separately, so the UI can tell "cancelled while queued" from
    // "cancel requested while running" instead of claiming success.
    const wasApplying = jobRun.phase === 'applying';
    const acknowledged = await requestJobCancel();
    // A cancel the backend acknowledged while the plan was merely prepared has
    // disposed that plan: no write was ever in flight, so there is nothing to
    // show a verdict for. Dropping it back to mapping is the honest outcome —
    // the next review mints a fresh planId. A cancel that could not be
    // addressed changes nothing and must leave the review untouched.
    if (acknowledged && !wasApplying) {
      resetJobRun();
      setPreview(null);
      setStep('mapping');
      setExecuteProgress('');
    }
  }, [jobRun.phase, requestJobCancel, resetJobRun]);

  /**
   * Once a plan is spent it is dead — the only legal recovery is a fresh
   * review that mints a new `planId`. `reprepare` drops the settled run so the
   * wizard cannot offer the spent plan again.
   */
  const handleReReviewPlan = useCallback(() => {
    resetJobRun();
    setPreview(null);
    setStep('mapping');
    setExecuteProgress('');
  }, [resetJobRun]);

  const stepIndex = STEPS.indexOf(step);
  const validBatchSize = Number.isInteger(batchSize) && batchSize >= 1 && batchSize <= 500;

  const canNext = useMemo(() => {
    switch (step) {
      case 'endpoints':
        return Boolean(
          sourceId &&
            sourceDatabase &&
            (destinationMode === 'sqlFile'
              ? sqlFileTarget
              : targetId && targetDatabase && pairing?.supported),
        );
      case 'setup':
        if (!validBatchSize) return false;
        if (writeMode !== 'insert' && !confirmedDestructive) return false;
        return true;
      case 'objects':
        return tables.length > 0 && tables.some((tbl) => tbl.enabled);
      case 'mapping':
        // Same predicate goNext re-checks after the prepare round trip.
        return mappingGateAllowsAdvance(tables);
      default:
        return false;
    }
  }, [
    step,
    sourceId,
    targetId,
    sourceDatabase,
    targetDatabase,
    destinationMode,
    sqlFileTarget,
    pairing,
    tables,
    writeMode,
    confirmedDestructive,
    validBatchSize,
  ]);

  const canExecute = useMemo(
    () =>
      preview?.canExecute === true &&
      (destinationMode === 'sqlFile' || !targetReadOnly) &&
      !loading,
    [preview, targetReadOnly, loading, destinationMode],
  );

  /**
   * A re-review may only be offered while nothing is pending
   * reconcile. A `null` verdict means no Job settled at all (a refusal before
   * any write), so there is no half-written range to protect and the affordance
   * stays open.
   */
  const reconcilePending = jobRun.verdict?.requiresReconcile ?? false;

  const goNext = useCallback(async () => {
    const next = STEPS[stepIndex + 1];
    if (step === 'mapping' && next === 'preview') {
      // The editor is disabled for the whole round trip, and the gate is
      // re-decided against the rows that exist *after* it. Deciding on a
      // click-time copy would let an inspect/refresh landing mid-flight move
      // the tables out from under an admitted plan.
      setPreparing(true);
      const admitted = await runPreview({ quiet: true }).finally(() => setPreparing(false));
      if (!admitted) {
        // Prepare itself refused; the preview step owns that error surface.
        setStep('preview');
        return;
      }
      if (!mappingGateAllowsAdvance(tablesRef.current)) {
        setMappingGateError(t('transfer.mapping.gateLost'));
        return;
      }
      setStep('preview');
      return;
    }
    if (next === 'objects' && tables.length === 0) {
      await runInspect();
    }
    if (next) setStep(next);
  }, [step, stepIndex, tables.length, runInspect, runPreview, destinationMode, t]);

  const goBack = () => {
    const prev = STEPS[stepIndex - 1];
    if (prev) setStep(prev);
  };

  const updateTable = useCallback((sourceTable: string, patch: Partial<TransferTableResult>) => {
    // Mapping edits, including a recordset change, invalidate the opaque
    // server preview immediately. The next preview must review the new scope.
    setPreview(null);
    // The user is re-answering the question the gate message asked.
    setMappingGateError('');
    setTables((prev) =>
      prev.map((tbl) => {
        if (tbl.sourceTable !== sourceTable) return tbl;
        const next = { ...tbl, ...patch };
        if (patch.columnMappings !== undefined) {
          next.ddlOverride = undefined;
        }
        return next;
      }),
    );
  }, []);

  const refreshTableMapping = useCallback(
    async (sourceTable: string) => {
      if (destinationMode === 'sqlFile') return;
      const { source, target } = await refreshEndpointSessions();
      const srcConnId = source?.dbSessionId;
      const tgtConnId = target?.dbSessionId;
      if (!srcConnId || !tgtConnId || !sourceDatabase || !targetDatabase) return;

      const payload = tablesToMappings();
      if (payload.length === 0) return;

      try {
        const rows = await transferCommands.inspect(
          srcConnId,
          tgtConnId,
          mode,
          sourceDatabase,
          targetDatabase,
          payload,
        );
        setTables((prev) =>
          prev.map((tbl) => {
            if (tbl.sourceTable !== sourceTable) return tbl;
            const inspected = rows.find((r) => r.sourceTable === sourceTable);
            if (!inspected) return tbl;
            return {
              ...tbl,
              status: inspected.status,
              targetColumns: inspected.targetColumns,
              sourceColumns: inspected.sourceColumns,
              incompatibleReason: inspected.incompatibleReason,
              createNew: tbl.createNew,
              targetTable: tbl.targetTable,
              columnMappings: tbl.columnMappings,
            };
          }),
        );
      } catch {
        // Keep local edits if refresh fails.
      }
    },
    [
      refreshEndpointSessions,
      sourceDatabase,
      targetDatabase,
      mode,
      tablesToMappings,
      destinationMode,
    ],
  );

  const toggleTable = (sourceTable: string) => {
    setTables((prev) =>
      prev.map((tbl) =>
        tbl.sourceTable === sourceTable ? { ...tbl, enabled: !tbl.enabled } : tbl,
      ),
    );
  };

  // All hooks above. Gate the body on the `sync` locale pack so the UI
  // never renders raw/un-translated keys before it is loaded.
  if (!localesReady) {
    return <LocaleDomainLoading testId="data-transfer-locale-loading" />;
  }

  // Built AFTER the `localesReady` gate on purpose: this is a plain
  // component-body array literal, so its four `t('transfer.mode.*')` calls run
  // on *every* render, used or not — the gate only stops the tree from being
  // rendered, and `useLocaleDomains` always reports not-ready on the first
  // frame. Building it above the gate made every fresh window open emit four
  // bogus dev-only "[i18n] Missing translation" lines for keys that are
  // perfectly well registered in the lazy `sync` pack. Keep it below the gate.
  const modeOptions: { value: TransferMode; label: string; hint: string; testId: string }[] = [
    {
      value: 'data',
      label: t('common.dataOnly'),
      hint: t('transfer.mode.dataHint'),
      testId: 'data-transfer-mode-data',
    },
    {
      value: 'structure',
      label: t('common.structureOnly'),
      hint: t('transfer.mode.structureHint'),
      testId: 'data-transfer-mode-structure',
    },
    {
      value: 'structureAndData',
      label: t('transfer.mode.both'),
      hint: t('transfer.mode.bothHint'),
      testId: 'data-transfer-mode-both',
    },
  ];

  return (
    <div data-testid="data-transfer-window" className="flex h-screen flex-col bg-surface text-fg">
      <TitleBar
        title={t('common.dataTransfer')}
        rightContent={<MigrationRunHistoryDialog operation="dataTransfer" />}
      />

      {migrationJobs.hydration?.verificationJobs.length ? (
        <div className="border-b border-edge px-6 py-2 text-xs text-amber-400">
          {t('migrationJob.pendingVerificationHint')}
        </div>
      ) : null}
      {migrationJobs.hydrationError ? (
        <div className="border-b border-edge px-6 py-2 text-xs text-amber-400">
          {migrationJobs.hydrationError}
        </div>
      ) : null}

      <div className="border-b border-edge px-6 py-3">
        <div className="mx-auto flex max-w-4xl flex-wrap items-center justify-center gap-1">
          {STEPS.map((s, i) => (
            <div key={s} className="flex items-center gap-1">
              {i > 0 && <ChevronRight className="h-3 w-3 shrink-0 text-fg-muted" aria-hidden />}
              <span
                data-testid={`data-transfer-step-${s}`}
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
                {t(`transfer.step.${s}`)}
              </span>
            </div>
          ))}
        </div>
      </div>

      <div className="flex min-h-0 flex-1 flex-col overflow-auto">
        <div
          className={cn(
            'mx-auto w-full flex-1 px-6 py-8',
            NARROW_STEPS.includes(step) ? 'max-w-2xl' : 'max-w-6xl',
          )}
        >
          {step === 'endpoints' && (
            <div className="space-y-4">
              <div className="flex flex-wrap items-end gap-2 rounded-lg border border-edge bg-surface-alt p-3">
                <label className="min-w-48 flex-1 text-xs">
                  <span className="mb-1 block text-fg-muted">{t('transfer.profile.name')}</span>
                  <Input
                    value={profileName}
                    onChange={(event) => setProfileName(event.target.value)}
                    placeholder={t('transfer.profile.namePlaceholder')}
                    data-testid="data-transfer-profile-name"
                    className="h-8 text-xs"
                  />
                </label>
                <Button
                  variant="secondary"
                  size="sm"
                  onClick={() => void saveCurrentProfile()}
                  data-testid="data-transfer-profile-save"
                >
                  {t('transfer.profile.save')}
                </Button>
                <label className="min-w-48 text-xs">
                  <span className="mb-1 block text-fg-muted">{t('transfer.profile.load')}</span>
                  <Select
                    value={selectedProfileId}
                    options={transferProfiles.map((profile) => ({
                      value: profile.id,
                      label: profile.name,
                    }))}
                    onChange={setSelectedProfileId}
                    placeholder={t('transfer.profile.select')}
                    triggerDataAttrs={{ 'data-testid': 'data-transfer-profile-select' }}
                  />
                </label>
                <Button
                  variant="ghost"
                  size="sm"
                  disabled={!selectedProfileId}
                  onClick={loadSelectedProfile}
                  data-testid="data-transfer-profile-load"
                >
                  {t('transfer.profile.load')}
                </Button>
              </div>
              <div className="flex gap-2 rounded-lg border border-edge bg-surface-alt p-2">
                <Button
                  variant={destinationMode === 'database' ? 'primary' : 'secondary'}
                  onClick={() => setDestinationMode('database')}
                  data-testid="data-transfer-destination-database"
                >
                  {t('transfer.destination.database')}
                </Button>
                <Button
                  variant={destinationMode === 'sqlFile' ? 'primary' : 'secondary'}
                  onClick={() => void chooseSqlFile()}
                  data-testid="data-transfer-destination-sql-file"
                >
                  {sqlFileTarget
                    ? t('transfer.destination.sqlFileSelected')
                    : t('transfer.destination.sqlFile')}
                </Button>
              </div>
              {destinationMode === 'database' ? (
                <MigrationEndpointsBar
                  layout="grid"
                  testIdPrefix="data-transfer"
                  i18nPrefix="transfer"
                  showSwap={false}
                  showCompare={false}
                  includeEmptyConnectionOption
                  hideDatabaseUntilConnected
                  sourceLabelKey="transfer.source"
                  targetLabelKey="transfer.target"
                  sourceId={sourceId}
                  targetId={targetId}
                  sourceDatabase={sourceDatabase}
                  targetDatabase={targetDatabase}
                  sourceDatabases={sourceDatabases}
                  targetDatabases={targetDatabases}
                  connOptions={connOptions}
                  targetOptions={targetOptions}
                  targetReadOnly={targetReadOnly}
                  onSourceChange={setSourceId}
                  onTargetChange={setTargetId}
                  onSourceDatabaseChange={setSourceDatabase}
                  onTargetDatabaseChange={setTargetDatabase}
                  footerNote={
                    pairing && !pairing.supported ? (
                      <TransferPairingNote reason={pairing.reason} />
                    ) : sourceConn &&
                      targetConn &&
                      !isVerifiedMigrationPair(sourceConn.databaseType, targetConn.databaseType) ? (
                      <span
                        data-testid="data-transfer-experimental-pair"
                        className="text-xs text-amber-600 dark:text-amber-400"
                      >
                        {t('common.experimentalPairHint')}
                      </span>
                    ) : undefined
                  }
                />
              ) : (
                <div className="space-y-3 rounded-lg border border-edge bg-surface-alt p-5">
                  <label className="block text-sm font-medium text-fg">
                    {t('transfer.destination.sourceDatabase')}
                  </label>
                  <Select
                    value={sourceId}
                    options={connOptions}
                    onChange={setSourceId}
                    placeholder={t('transfer.selectSource')}
                    triggerDataAttrs={{ 'data-testid': 'data-transfer-source' }}
                  />
                  <Select
                    value={sourceDatabase}
                    options={sourceDatabases.map((db) => ({ value: db, label: db }))}
                    onChange={setSourceDatabase}
                    placeholder={t('transfer.selectDatabase')}
                    triggerDataAttrs={{ 'data-testid': 'data-transfer-source-database' }}
                  />
                  <label className="block text-sm font-medium text-fg">
                    {t('transfer.destination.sqlDialect')}
                  </label>
                  <Select
                    value={sqlFileDialect}
                    options={[
                      {
                        value: 'source',
                        label: t('transfer.destination.sourceDialect', {
                          dialect: sourceConn?.databaseType ?? 'source',
                        }),
                      },
                      ...availableSqlDialects
                        .filter((driver) => driver !== sourceConn?.databaseType)
                        .map((driver) => ({
                          value: driver,
                          label: DB_REGISTRY[driver as keyof typeof DB_REGISTRY]?.label ?? driver,
                        })),
                    ]}
                    onChange={(value) => {
                      setSqlFileDialect(value);
                      setPreview(null);
                      // Target-native type overrides are dialect-specific;
                      // force a fresh source-only inspection before preview.
                      setTables([]);
                    }}
                    placeholder={t('transfer.destination.sourceDialect', {
                      dialect: sourceConn?.databaseType ?? 'source',
                    })}
                    triggerDataAttrs={{ 'data-testid': 'data-transfer-sql-file-dialect' }}
                  />
                  <p className="text-xs text-fg-muted">
                    {t('transfer.destination.sqlDialectHint')}
                  </p>
                  <label className="block text-sm">
                    <span className="text-xs font-medium text-fg">
                      {t('transfer.destination.sqlEncoding')}
                    </span>
                    <Select
                      value={sqlFileEncoding}
                      options={[
                        {
                          value: 'utf8',
                          label: t('transfer.destination.sqlEncodingUtf8'),
                        },
                        {
                          value: 'utf8Bom',
                          label: t('transfer.destination.sqlEncodingUtf8Bom'),
                        },
                        {
                          value: 'utf16Le',
                          label: t('transfer.destination.sqlEncodingUtf16Le'),
                        },
                        {
                          value: 'utf16Be',
                          label: t('transfer.destination.sqlEncodingUtf16Be'),
                        },
                      ]}
                      onChange={(value) => {
                        setSqlFileEncoding(value as 'utf8' | 'utf8Bom' | 'utf16Le' | 'utf16Be');
                        setPreview(null);
                      }}
                      triggerDataAttrs={{ 'data-testid': 'data-transfer-sql-file-encoding' }}
                    />
                  </label>
                  <label className="block text-sm">
                    <span className="text-xs font-medium text-fg">
                      {t('transfer.destination.sqlCompression')}
                    </span>
                    <Select
                      value={sqlFileCompression}
                      options={[
                        {
                          value: 'none',
                          label: t('transfer.destination.sqlCompressionNone'),
                        },
                        {
                          value: 'gzip',
                          label: t('transfer.destination.sqlCompressionGzip'),
                        },
                      ]}
                      onChange={(value) => {
                        setSqlFileCompression(value as 'none' | 'gzip');
                        setPreview(null);
                      }}
                      triggerDataAttrs={{ 'data-testid': 'data-transfer-sql-file-compression' }}
                    />
                  </label>
                  <div className="grid grid-cols-2 gap-3 border-t border-edge pt-3">
                    <label className="block text-sm">
                      <span className="text-xs font-medium text-fg">
                        {t('transfer.destination.targetDatabase')}
                      </span>
                      <Input
                        value={sqlFileDatabase}
                        onChange={(event) => {
                          setSqlFileDatabase(event.target.value);
                          setPreview(null);
                        }}
                        placeholder={t('transfer.destination.targetDatabasePlaceholder')}
                        data-testid="data-transfer-sql-file-target-database"
                        className="mt-1 h-8 text-xs"
                      />
                    </label>
                    <label className="block text-sm">
                      <span className="text-xs font-medium text-fg">
                        {t('transfer.destination.targetSchema')}
                      </span>
                      <Input
                        value={sqlFileSchema}
                        onChange={(event) => {
                          setSqlFileSchema(event.target.value);
                          setPreview(null);
                        }}
                        placeholder={t('transfer.destination.targetSchemaPlaceholder')}
                        data-testid="data-transfer-sql-file-target-schema"
                        className="mt-1 h-8 text-xs"
                      />
                    </label>
                  </div>
                  <p className="text-xs text-fg-muted">
                    {t('transfer.destination.targetScopeHint')}
                  </p>
                  <p className="text-xs text-fg-muted">
                    {sqlFileTarget
                      ? t('transfer.destination.sqlFileHint')
                      : t('transfer.destination.chooseHint')}
                  </p>
                </div>
              )}
            </div>
          )}

          {step === 'setup' && (
            <div className="space-y-6 rounded-lg border border-edge bg-surface-alt p-6">
              <div className="space-y-2">
                <p className="text-sm font-medium text-fg">{t('transfer.setup.modeSection')}</p>
                {modeOptions.map((opt) => (
                  <label
                    key={opt.value}
                    className={cn(
                      'flex cursor-pointer flex-col rounded-lg border px-4 py-3 transition-colors',
                      mode === opt.value
                        ? 'border-accent bg-surface ring-1 ring-accent'
                        : 'border-edge bg-surface hover:border-edge/80',
                    )}
                  >
                    <span className="flex items-center gap-2 text-sm font-medium">
                      <Radio
                        name="transfer-mode"
                        checked={mode === opt.value}
                        onChange={() => setMode(opt.value)}
                        data-testid={opt.testId}
                      />
                      {opt.label}
                    </span>
                    <span className="mt-1 pl-6 text-xs text-fg-muted">{opt.hint}</span>
                  </label>
                ))}
              </div>
              <div className="space-y-3 border-t border-edge pt-4">
                <p className="text-[11px] font-semibold uppercase tracking-wider text-fg-muted">
                  {t('transfer.setup.optionsSection')}
                </p>
                {(mode !== 'data' || writeMode === 'dropCreateInsert') && (
                  <label className="flex items-start gap-2 rounded border border-warning/40 bg-warning/5 p-2 text-xs">
                    <Checkbox
                      className="mt-0.5"
                      checked={useTargetDefaultCollation}
                      onChange={(event) => setUseTargetDefaultCollation(event.target.checked)}
                      data-testid="data-transfer-use-target-default-collation"
                    />
                    <span>
                      <span className="block font-medium text-fg">
                        {t('transfer.mapping.targetDefaultCollation')}
                      </span>
                      <span className="text-fg-muted">
                        {t('transfer.mapping.targetDefaultCollationHint')}
                      </span>
                    </span>
                  </label>
                )}
                <label className="block text-sm">
                  {t('transfer.writeMode.label')}
                  <div className="mt-1" data-testid="data-transfer-write-mode">
                    <Select
                      value={writeMode}
                      onChange={(v) => setWriteMode(v as WriteMode)}
                      options={[
                        { value: 'insert', label: t('transfer.writeMode.insert') },
                        { value: 'truncateInsert', label: t('transfer.writeMode.truncateInsert') },
                        { value: 'dropCreateInsert', label: t('transfer.writeMode.dropCreate') },
                      ]}
                    />
                  </div>
                </label>
                {writeMode !== 'insert' && (
                  <label className="flex items-center gap-2 text-sm text-warning">
                    <Checkbox
                      checked={confirmedDestructive}
                      onChange={(e) => setConfirmedDestructive(e.target.checked)}
                      data-testid="data-transfer-destructive-confirm"
                    />
                    {t('transfer.destructiveConfirm')}
                  </label>
                )}
                <label className="block text-sm">
                  {t('transfer.batchSize')}
                  <Input
                    type="number"
                    min={1}
                    max={500}
                    step={1}
                    aria-invalid={!validBatchSize}
                    data-testid="data-transfer-batch-size"
                    className="mt-1"
                    value={batchSize}
                    onChange={(e) => setBatchSize(Number(e.target.value))}
                  />
                </label>
                {!validBatchSize ? (
                  <ErrorBanner as="p" data-testid="data-transfer-batch-size-error">
                    {t('transfer.batchSizeLimit')}
                  </ErrorBanner>
                ) : null}
                <p
                  className="text-xs text-fg-muted"
                  role="note"
                  data-testid="data-transfer-resume-capability-hint"
                >
                  {t('transfer.resumeCapabilityHint')}
                </p>
                <label className="flex items-center gap-2 text-sm">
                  <Checkbox
                    checked={stopOnError}
                    onChange={(e) => setStopOnError(e.target.checked)}
                  />
                  {t('transfer.stopOnError')}
                </label>
              </div>
            </div>
          )}

          {step === 'objects' && (
            <div>
              {loading ? (
                <div className="flex justify-center py-12">
                  <Spinner size="2xl" tone="accent" />
                </div>
              ) : tables.length === 0 ? (
                <div
                  data-testid="data-transfer-objects-empty"
                  className="rounded-lg border border-edge bg-surface-alt px-4 py-8 text-center text-sm"
                >
                  <p className="font-medium text-fg">{t('transfer.objects.noTablesFound')}</p>
                  <p className="mt-2 text-fg-muted">{t('transfer.objects.noTablesHint')}</p>
                  <Button
                    variant="primary"
                    size="sm"
                    className="mt-4"
                    data-testid="data-transfer-reinspect"
                    onClick={() => void runInspect()}
                  >
                    {t('transfer.objects.reInspect')}
                  </Button>
                </div>
              ) : (
                <ul className="divide-y divide-edge overflow-hidden rounded-lg border border-edge bg-surface-alt">
                  {tables.map((tbl) => (
                    <li
                      key={tbl.sourceTable}
                      data-testid="data-transfer-table-row"
                      className="flex items-center gap-2 px-3 py-2 text-sm"
                    >
                      <Checkbox
                        checked={tbl.enabled}
                        onChange={() => toggleTable(tbl.sourceTable)}
                      />
                      <span className="flex-1 font-mono text-xs">{tbl.sourceTable}</span>
                      <span className="text-fg-muted">→ {tbl.targetTable || '—'}</span>
                      <span className="text-xs uppercase text-fg-muted">{tbl.status}</span>
                    </li>
                  ))}
                </ul>
              )}
            </div>
          )}

          {step === 'mapping' && (
            <TransferMappingStep
              tables={tables}
              selectedSourceTable={selectedMappingTable}
              mode={mode}
              disabled={preparing}
              onSelectTable={setSelectedMappingTable}
              onUpdateTable={updateTable}
              onTargetTableCommit={(sourceTable) => void refreshTableMapping(sourceTable)}
            />
          )}

          <MappingGateNotice
            visible={step === 'mapping'}
            rows={tables}
            gateLost={mappingGateError}
          />

          {step === 'preview' && !loading && !preview && (
            <div
              data-testid="data-transfer-preview-error"
              className="rounded-lg border border-edge bg-surface-alt px-4 py-8 text-center text-sm"
            >
              <p className="font-medium text-fg">{t('transfer.preview.failed')}</p>
              <CopyableError
                message={previewError || t('transfer.preview.failedHint')}
                className="error-message mx-auto mt-3 max-w-xl text-left text-xs"
                copyButton
              />
              <div className="mt-4 flex flex-wrap items-center justify-center gap-2">
                <Button
                  variant="primary"
                  size="sm"
                  data-testid="data-transfer-preview-retry"
                  onClick={() => void runPreview({ quiet: true })}
                >
                  {t('transfer.preview.retry')}
                </Button>
                <Button
                  variant="ghost"
                  size="sm"
                  data-testid="data-transfer-preview-back-mapping"
                  onClick={goBack}
                >
                  {t('transfer.preview.backToMapping')}
                </Button>
              </div>
            </div>
          )}

          {step === 'preview' && loading && (
            <div className="flex justify-center py-12">
              <Spinner size="2xl" tone="accent" />
            </div>
          )}

          {step === 'preview' && preview && (
            <div data-testid="data-transfer-preview" className="relative space-y-3 text-sm">
              {executing && (
                <div
                  data-testid="data-transfer-executing-overlay"
                  className="absolute inset-0 z-10 flex flex-col items-center justify-center gap-2 rounded-lg bg-surface/90 text-sm text-fg-muted"
                >
                  <Spinner size="2xl" tone="accent" />
                  <span>{executeProgress || t('transfer.executing')}</span>
                </div>
              )}
              {preview.blockReason && (
                <p className="rounded border border-warning/30 bg-warning/10 px-3 py-2 select-text text-warning">
                  {preview.blockReason}
                </p>
              )}
              {preview.ddl.map((item, ddlIndex) => {
                const table = tables.find((row) => row.sourceTable === item.sourceTable);
                const itemKind = item.kind ?? 'table';
                const ddlValue =
                  destinationMode === 'sqlFile' || itemKind !== 'table'
                    ? item.ddl
                    : (table?.ddlOverride ?? item.ddl);
                const kindLabel =
                  itemKind === 'index'
                    ? t('transfer.ddlKind.index')
                    : itemKind === 'foreignKey'
                      ? t('transfer.ddlKind.foreignKey')
                      : itemKind === 'dropTable'
                        ? t('transfer.ddlKind.dropTable')
                        : null;
                const ddlLabel = kindLabel
                  ? `${item.sourceTable} → ${item.targetTable} (${kindLabel})`
                  : `${item.sourceTable} → ${item.targetTable}`;
                const rowKey = `${item.sourceTable}-${itemKind}-${ddlIndex}`;
                return (
                  <div
                    key={rowKey}
                    className="overflow-hidden rounded-lg border border-edge bg-surface-alt"
                  >
                    <div className="flex items-center justify-between gap-2 border-b border-edge px-3 py-1.5 text-xs text-fg-muted">
                      <span>{ddlLabel}</span>
                      <Button
                        variant="ghost"
                        size="sm"
                        data-testid={`data-transfer-copy-ddl-${item.sourceTable}${itemKind === 'table' ? '' : `-${itemKind}`}`}
                        onClick={() => void navigator.clipboard.writeText(ddlValue)}
                      >
                        {t('common.copyDdl')}
                      </Button>
                    </div>
                    {destinationMode === 'sqlFile' || itemKind !== 'table' ? (
                      <pre
                        className="max-h-64 min-h-[12rem] overflow-auto whitespace-pre-wrap bg-surface p-3 font-mono text-xs"
                        data-testid={`data-transfer-ddl-preview-${item.sourceTable}${destinationMode === 'sqlFile' && itemKind === 'table' ? '' : `-${itemKind}-${ddlIndex}`}`}
                      >
                        {ddlValue}
                      </pre>
                    ) : (
                      <>
                        <div
                          className="h-48 min-h-[12rem] bg-surface"
                          data-testid={`data-transfer-ddl-editor-${item.sourceTable}`}
                        >
                          <SqlCodeBlock
                            code={ddlValue}
                            dialect={targetConn?.databaseType ?? 'mysql'}
                            onChange={(next) => updateTableDdlOverride(item.sourceTable, next)}
                          />
                        </div>
                        <p className="border-t border-edge px-3 py-1.5 text-[11px] text-fg-muted">
                          {t('transfer.ddlOverrideHint')}
                        </p>
                      </>
                    )}
                  </div>
                );
              })}
              {preview.writePlans.map((plan) => (
                <div
                  key={plan.sourceTable}
                  className="rounded-lg border border-edge bg-surface-alt p-3"
                >
                  <div>
                    {plan.sourceTable} → {plan.targetTable} ({plan.writeMode})
                  </div>
                  <div className="text-fg-muted">
                    {t('transfer.estimatedRows')}: {plan.estimatedRows ?? '—'}
                  </div>
                  {plan.sourceFilterPreview && (
                    <div className="mt-1 text-xs text-fg-muted">
                      {t('transfer.sourceFilterPreview')}: <code>{plan.sourceFilterPreview}</code>
                    </div>
                  )}
                  {plan.recordsetPreview && (
                    <div className="mt-1 text-xs text-fg-muted">
                      {t('transfer.recordsetPreview')}: <code>{plan.recordsetPreview}</code>
                    </div>
                  )}
                </div>
              ))}
              {preview.warnings.map((w) => (
                <p key={w} className="text-xs text-fg-muted">
                  {w}
                </p>
              ))}
            </div>
          )}

          {step === 'result' && jobRun.applyView && (
            <TransferJobResultPanel
              run={jobRun}
              onReReview={handleReReviewPlan}
              testIdPrefix="data-transfer-job"
            />
          )}

          {step === 'preview' && jobRun.cancelRequested && (
            <p
              role="status"
              data-testid="data-transfer-cancel-disposition"
              data-cancel-acknowledged={String(jobRun.cancelAcknowledged)}
              data-cancel-unknown-job={String(jobRun.cancelUnknownJob)}
              className="text-sm text-fg-muted"
            >
              {jobRun.cancelUnknownJob
                ? t('migration.cancel.unknownJob')
                : t('migration.cancel.requestedInFlight')}
            </p>
          )}

          {/*
            A refusal never reaches the result step — `runExecute` stays on
            `preview` so the fail-closed notices (scope, budget, consumed plan)
            are shown where the user still has the review in front of them.
          */}
          {step === 'preview' && jobRun.failure && (
            <MigrationJobFailureNotice
              failure={jobRun.failure}
              onReReview={handleReReviewPlan}
              testIdPrefix="data-transfer-job"
            />
          )}
        </div>
      </div>

      <div className="flex shrink-0 items-center justify-between border-t border-edge px-6 py-3">
        <Button variant="ghost" disabled={stepIndex === 0 || executing} onClick={goBack}>
          <ChevronLeft className="h-4 w-4" /> {t('transfer.back')}
        </Button>
        <div className="flex items-center gap-2">
          {/* A cancel is offered for as long as there is an admitted plan to
              abandon. While an apply is in flight the server owns the Job id
              and the control is present but not addressable. */}
          {step === 'preview' && jobRun.prepareView !== null && (
            <Button
              variant="ghost"
              data-testid="data-transfer-cancel"
              data-cancel-addressable={String(jobRun.cancelTargetJobId !== null)}
              disabled={jobRun.cancelTargetJobId === null}
              onClick={() => void handleCancel()}
            >
              {t('transfer.cancel')}
            </Button>
          )}
          {step === 'preview' && executing && (
            <span className="text-sm text-fg-muted">
              {executeProgress || t('transfer.executing')}
            </span>
          )}
          {/*
            A cancel click is a *request*, not a cancellation, and it can
            only be addressed to a Job id the frontend actually holds. The
            server mints the Job id and returns it with the terminal view, so
            while `apply_data_transfer_job` is still in flight there is no id
            to send — say so instead of pretending the run can be stopped.
          */}
          {step === 'preview' && executing && jobRun.cancelTargetJobId === null && (
            <p role="status" className="text-sm text-fg-muted" data-testid="data-transfer-cancel-pending-id">
              {t('migration.cancel.unknownJob')}
            </p>
          )}
          {step === 'preview' ? (
            <Button
              variant="run"
              data-testid="data-transfer-execute"
              disabled={!canExecute || executing}
              onClick={handleExecuteClick}
            >
              {executing ? <Spinner size="lg" /> : null}
              {executing ? t('transfer.executing') : t('transfer.execute')}
            </Button>
          ) : step === 'result' ? (
            // The Job contract has no resume token, so the only legal
            // next action after a settled run is a fresh review that mints a new
            // planId — a spent plan is never replayed under its old id. That is
            // also exactly the pending-reconcile hazard: while a reconcile is still
            // pending the first run's stopping point is unknown, so one click
            // here would be a second write over the same range. The affordance
            // is therefore closed until the target has been verified read-only;
            // the panel above names the reason, and reopening the window starts a
            // genuinely fresh review.
            <>
              <Button
                variant="run"
                data-testid="data-transfer-rereview"
                data-blocked={reconcilePending ? 'reconcile-pending' : 'false'}
                disabled={reconcilePending}
                onClick={handleReReviewPlan}
              >
                {t('migration.verdict.rereview')}
              </Button>
              {reconcilePending && (
                <p role="status" data-testid="data-transfer-rereview-blocked" className="text-sm text-fg-muted">
                  {t('migration.verdict.rereviewBlocked')}
                </p>
              )}
            </>
          ) : (
            <Button
              data-testid="data-transfer-next"
              disabled={!canNext || loading}
              onClick={() => void goNext()}
            >
              {loading ? <Spinner size="lg" /> : t('transfer.next')}
              <ChevronRight className="h-4 w-4" />
            </Button>
          )}
        </div>
      </div>

      <StatusBar />

      <LimitationsDialog
        open={limitationsOpen}
        onClose={() => setLimitationsOpen(false)}
        titleKey="transfer.limitations.title"
        dontShowAgainKey="transfer.limitations.dontShowAgain"
        limitationKeys={TRANSFER_LIMITATION_KEYS}
        testIdPrefix="data-transfer"
        onDismiss={setTransferLimitationsDismissed}
      />

      <TransferExecuteConfirmDialog
        open={executeConfirmOpen}
        writeMode={writeMode}
        writePlans={preview?.writePlans ?? []}
        onClose={() => setExecuteConfirmOpen(false)}
        onConfirm={handleExecuteConfirm}
      />

      <Dialog
        open={errorOpen}
        onClose={() => setErrorOpen(false)}
        title={t('transfer.error')}
        footer={
          <Button variant="primary" onClick={() => setErrorOpen(false)}>
            {t('common.ok')}
          </Button>
        }
      >
        <CopyableError
          message={errorMsg}
          className="error-message text-sm"
          copyButton
          data-testid="data-transfer-error"
        />
      </Dialog>
    </div>
  );
}
