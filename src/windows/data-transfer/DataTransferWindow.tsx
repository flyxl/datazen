import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { ChevronLeft, ChevronRight, Loader2 } from 'lucide-react';
import { TitleBar } from '../../components/TitleBar';
import { StatusBar } from '../../components/StatusBar';
import { LocaleDomainLoading } from '../../components/LocaleDomainLoading';
import { Button } from '../../components/ui/Button';
import { Dialog } from '../../components/ui/Dialog';
import { CopyableError } from '../../components/ui/CopyableError';
import { Select } from '../../components/ui/Select';
import {
  DEFAULT_TRANSFER_OPTIONS,
  transferCommands,
  type TransferJob,
  type TransferMode,
  type TransferPreview,
  type TransferTableMapping,
  type TransferTableResult,
  type TransferExecutionResult,
  type WriteMode,
} from '../../commands/transfer';
import { useI18n } from '../../hooks/useI18n';
import { useLocaleDomains } from '../../hooks/useLocaleDomains';
import { useSettings } from '../../hooks/useSettings';
import { useSettingsStore } from '../../stores/settingsStore';
import { cn } from '../../lib/cn';
import { listenCrossWindow } from '../../lib/crossWindowBus';
import {
  isTransferLimitationsDismissed,
  setTransferLimitationsDismissed,
} from '../../lib/transferLimitationsPrefs';
import { isTransferTargetSupported, resolveTransferPairing } from '../../lib/transferPairing';
import type { ConnectionConfig } from '../../types';
import { LimitationsDialog } from '../../components/ui/LimitationsDialog';
import { TRANSFER_LIMITATION_KEYS } from './transferLimitationKeys';
import { TransferExecuteConfirmDialog } from './TransferExecuteConfirmDialog';
import { TransferMappingStep } from './TransferMappingStep';
import {
  MigrationEndpointsBar,
  TransferPairingNote,
} from '../../components/migration/MigrationEndpointsBar';
import { normalizeColumnMappings, tableHasActiveMappings } from './transferMappingView';
import { SqlCodeBlock } from '../../components/SqlCodeBlock';
import {
  ensureDedicatedSession,
  listDatabasesDedicated,
  releaseDedicatedSession,
  type DedicatedSideSession,
} from '../../lib/dedicatedDbSession';

type WizardStep = 'endpoints' | 'setup' | 'objects' | 'mapping' | 'preview' | 'result';

const STEPS: WizardStep[] = ['endpoints', 'setup', 'objects', 'mapping', 'preview', 'result'];

const NARROW_STEPS: WizardStep[] = ['endpoints', 'setup', 'result'];

export function DataTransferWindow() {
  const localesReady = useLocaleDomains(['transfer']);
  useSettings();
  const { t } = useI18n();
  const loadSettings = useSettingsStore((s) => s.loadSettings);

  const [connections, setConnections] = useState<ConnectionConfig[]>([]);
  const [sourceSession, setSourceSession] = useState<DedicatedSideSession | null>(null);
  const [targetSession, setTargetSession] = useState<DedicatedSideSession | null>(null);
  const [sourceId, setSourceId] = useState('');
  const [targetId, setTargetId] = useState('');
  const [sourceDatabases, setSourceDatabases] = useState<string[]>([]);
  const [targetDatabases, setTargetDatabases] = useState<string[]>([]);
  const [sourceDatabase, setSourceDatabase] = useState('');
  const [targetDatabase, setTargetDatabase] = useState('');
  const [mode, setMode] = useState<TransferMode>('data');
  const [writeMode, setWriteMode] = useState<WriteMode>('insert');
  const [tables, setTables] = useState<TransferTableResult[]>([]);
  const [preview, setPreview] = useState<TransferPreview | null>(null);
  const [result, setResult] = useState<TransferExecutionResult | null>(null);
  const [step, setStep] = useState<WizardStep>('endpoints');
  const [batchSize, setBatchSize] = useState(DEFAULT_TRANSFER_OPTIONS.batchSize ?? 500);
  const [stopOnError, setStopOnError] = useState(true);
  const [confirmedDestructive, setConfirmedDestructive] = useState(false);
  const [loading, setLoading] = useState(false);
  const [executing, setExecuting] = useState(false);
  const [executeProgress, setExecuteProgress] = useState('');
  const [errorMsg, setErrorMsg] = useState('');
  const [errorOpen, setErrorOpen] = useState(false);
  const [previewError, setPreviewError] = useState('');
  const [limitationsOpen, setLimitationsOpen] = useState(false);
  const [executeConfirmOpen, setExecuteConfirmOpen] = useState(false);
  const [selectedMappingTable, setSelectedMappingTable] = useState('');
  const jobIdRef = useRef<string | null>(null);

  useEffect(() => {
    void loadSettings();
  }, [loadSettings]);

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
    const srcType = sourceConn?.databaseType;
    return connections.map((c) => {
      const unsupported = Boolean(srcType && !isTransferTargetSupported(srcType, c.databaseType));
      const base = `${c.name} (${c.databaseType})`;
      return {
        value: c.id,
        label: unsupported ? `${base} — ${hint}` : base,
        disabled: unsupported,
        title: unsupported ? hint : undefined,
      };
    });
  }, [connections, sourceConn?.databaseType, t]);

  const pairing = useMemo(() => {
    if (!sourceConn || !targetConn) return null;
    return resolveTransferPairing(sourceConn.databaseType, targetConn.databaseType);
  }, [sourceConn, targetConn]);

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
        setSourceDatabases(databases);
        const preferred = cfg?.database ?? '';
        setSourceDatabase((prev) =>
          databases.includes(preferred)
            ? preferred
            : prev && databases.includes(prev)
              ? prev
              : (databases[0] ?? ''),
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
        setTargetDatabases(databases);
        const preferred = cfg?.database ?? '';
        setTargetDatabase((prev) =>
          databases.includes(preferred)
            ? preferred
            : prev && databases.includes(prev)
              ? prev
              : (databases[0] ?? ''),
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

  const tablesToMappings = useCallback(
    (): TransferTableMapping[] =>
      tables
        .filter((tbl) => tbl.enabled && tbl.sourceTable)
        .map((tbl) => ({
          sourceTable: tbl.sourceTable,
          targetTable: tbl.targetTable,
          createNew: tbl.createNew,
          enabled: tbl.enabled,
          columnMappings: normalizeColumnMappings(tbl),
          ddlOverride: tbl.ddlOverride?.trim() ? tbl.ddlOverride.trim() : undefined,
        })),
    [tables],
  );

  const updateTableDdlOverride = useCallback((sourceTable: string, ddl: string) => {
    setTables((prev) =>
      prev.map((row) => (row.sourceTable === sourceTable ? { ...row, ddlOverride: ddl } : row)),
    );
  }, []);

  const refreshEndpointSessions = useCallback(async () => {
    if (!sourceId || !targetId || !sourceDatabase || !targetDatabase) {
      return {
        source: null as DedicatedSideSession | null,
        target: null as DedicatedSideSession | null,
      };
    }
    const [source, target] = await Promise.all([
      ensureDedicatedSession(sourceSession, sourceId, sourceDatabase),
      ensureDedicatedSession(targetSession, targetId, targetDatabase),
    ]);
    setSourceSession(source);
    setTargetSession(target);
    return { source, target };
  }, [sourceSession, targetSession, sourceId, targetId, sourceDatabase, targetDatabase]);

  const buildJob = useCallback(
    (sessions?: {
      source: DedicatedSideSession | null;
      target: DedicatedSideSession | null;
    }): TransferJob | null => {
      const srcConnId = sessions?.source?.dbSessionId ?? sourceSession?.dbSessionId;
      const tgtConnId = sessions?.target?.dbSessionId ?? targetSession?.dbSessionId;
      if (!srcConnId || !tgtConnId || !sourceDatabase || !targetDatabase) return null;
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
        },
      };
    },
    [
      sourceSession?.dbSessionId,
      targetSession?.dbSessionId,
      sourceDatabase,
      targetDatabase,
      mode,
      writeMode,
      tables,
      tablesToMappings,
      batchSize,
      stopOnError,
      confirmedDestructive,
    ],
  );

  const runInspect = useCallback(async () => {
    const { source, target } = await refreshEndpointSessions();
    const srcConnId = source?.dbSessionId;
    const tgtConnId = target?.dbSessionId;
    if (!srcConnId || !tgtConnId || !sourceDatabase || !targetDatabase) {
      setErrorMsg(t('transfer.selectBoth'));
      setErrorOpen(true);
      return;
    }
    setLoading(true);
    try {
      const rows = await transferCommands.inspect(
        srcConnId,
        tgtConnId,
        mode,
        sourceDatabase,
        targetDatabase,
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
    } catch (e) {
      setErrorMsg(e instanceof Error ? e.message : String(e));
      setErrorOpen(true);
    } finally {
      setLoading(false);
    }
  }, [refreshEndpointSessions, sourceDatabase, targetDatabase, mode, t]);

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
      setLoading(true);
      try {
        const p = await transferCommands.preview(job);
        setPreview(p);
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
    [refreshEndpointSessions, buildJob, t],
  );

  const runExecute = useCallback(async () => {
    const sessions = await refreshEndpointSessions();
    const job = buildJob(sessions);
    if (!job) return;
    if (targetReadOnly) {
      setErrorMsg(t('transfer.readOnlyBlock'));
      setErrorOpen(true);
      return;
    }
    const jobId = crypto.randomUUID();
    jobIdRef.current = jobId;
    setExecuting(true);
    const tableCount = job.tables.filter((tbl) => tbl.enabled).length;
    setExecuteProgress(t('transfer.executingProgress', { count: tableCount }));
    try {
      const execResult = await transferCommands.execute(job, jobId);
      setResult(execResult);
      setStep('result');
    } catch (e) {
      setErrorMsg(e instanceof Error ? e.message : String(e));
      setErrorOpen(true);
    } finally {
      setExecuting(false);
      setExecuteProgress('');
      jobIdRef.current = null;
    }
  }, [refreshEndpointSessions, buildJob, targetReadOnly, t]);

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
    const id = jobIdRef.current;
    if (id) await transferCommands.cancel(id).catch(() => {});
  }, []);

  const stepIndex = STEPS.indexOf(step);

  const canNext = useMemo(() => {
    switch (step) {
      case 'endpoints':
        return Boolean(
          sourceId && targetId && sourceDatabase && targetDatabase && pairing?.supported,
        );
      case 'setup':
        if (writeMode !== 'insert' && !confirmedDestructive) return false;
        return true;
      case 'objects':
        return tables.length > 0 && tables.some((tbl) => tbl.enabled);
      case 'mapping':
        return tables.some((tbl) => tbl.enabled && tableHasActiveMappings(tbl));
      default:
        return false;
    }
  }, [
    step,
    sourceId,
    targetId,
    sourceDatabase,
    targetDatabase,
    pairing,
    tables,
    writeMode,
    confirmedDestructive,
  ]);

  const canExecute = useMemo(
    () => preview?.canExecute === true && !targetReadOnly && !loading,
    [preview, targetReadOnly, loading],
  );

  const goNext = useCallback(async () => {
    const next = STEPS[stepIndex + 1];
    if (step === 'mapping' && next === 'preview') {
      await runPreview({ quiet: true });
      setStep('preview');
      return;
    }
    if (next === 'objects' && tables.length === 0) {
      await runInspect();
    }
    if (next) setStep(next);
  }, [step, stepIndex, tables.length, runInspect, runPreview]);

  const goBack = () => {
    const prev = STEPS[stepIndex - 1];
    if (prev) setStep(prev);
  };

  const updateTable = useCallback((sourceTable: string, patch: Partial<TransferTableResult>) => {
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
    [refreshEndpointSessions, sourceDatabase, targetDatabase, mode, tablesToMappings],
  );

  const toggleTable = (sourceTable: string) => {
    setTables((prev) =>
      prev.map((tbl) =>
        tbl.sourceTable === sourceTable ? { ...tbl, enabled: !tbl.enabled } : tbl,
      ),
    );
  };

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

  // All hooks above. Gate the body on the `transfer` locale pack so the UI
  // never renders raw/un-translated `t('transfer.*')` keys before it is loaded.
  if (!localesReady) {
    return <LocaleDomainLoading testId="data-transfer-locale-loading" />;
  }

  return (
    <div data-testid="data-transfer-window" className="flex h-screen flex-col bg-surface text-fg">
      <TitleBar title={t('common.dataTransfer')} />

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
                      ? 'bg-accent text-white'
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
            <MigrationEndpointsBar
              layout="grid"
              testIdPrefix="data-transfer"
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
                ) : undefined
              }
            />
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
                      <input
                        type="radio"
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
                    <input
                      type="checkbox"
                      checked={confirmedDestructive}
                      onChange={(e) => setConfirmedDestructive(e.target.checked)}
                      data-testid="data-transfer-destructive-confirm"
                    />
                    {t('transfer.destructiveConfirm')}
                  </label>
                )}
                <label className="block text-sm">
                  {t('transfer.batchSize')}
                  <input
                    type="number"
                    min={1}
                    className="mt-1 w-full rounded border border-edge bg-surface px-2 py-1"
                    value={batchSize}
                    onChange={(e) => setBatchSize(Number(e.target.value) || 500)}
                  />
                </label>
                <label className="flex items-center gap-2 text-sm">
                  <input
                    type="checkbox"
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
                  <Loader2 className="h-6 w-6 animate-spin text-accent" />
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
                      <input
                        type="checkbox"
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
              onSelectTable={setSelectedMappingTable}
              onUpdateTable={updateTable}
              onTargetTableCommit={(sourceTable) => void refreshTableMapping(sourceTable)}
            />
          )}

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
              <Loader2 className="h-6 w-6 animate-spin text-accent" />
            </div>
          )}

          {step === 'preview' && preview && (
            <div data-testid="data-transfer-preview" className="relative space-y-3 text-sm">
              {executing && (
                <div
                  data-testid="data-transfer-executing-overlay"
                  className="absolute inset-0 z-10 flex flex-col items-center justify-center gap-2 rounded-lg bg-surface/90 text-sm text-fg-muted"
                >
                  <Loader2 className="h-6 w-6 animate-spin text-accent" />
                  <span>{executeProgress || t('transfer.executing')}</span>
                </div>
              )}
              {preview.blockReason && (
                <p className="rounded border border-warning/30 bg-warning/10 px-3 py-2 select-text text-warning">
                  {preview.blockReason}
                </p>
              )}
              {preview.ddl.map((item) => {
                const table = tables.find((row) => row.sourceTable === item.sourceTable);
                const ddlValue = table?.ddlOverride ?? item.ddl;
                return (
                  <div
                    key={item.sourceTable}
                    className="overflow-hidden rounded-lg border border-edge bg-surface-alt"
                  >
                    <div className="flex items-center justify-between gap-2 border-b border-edge px-3 py-1.5 text-xs text-fg-muted">
                      <span>
                        {item.sourceTable} → {item.targetTable}
                      </span>
                      <Button
                        variant="ghost"
                        size="sm"
                        data-testid={`data-transfer-copy-ddl-${item.sourceTable}`}
                        onClick={() => void navigator.clipboard.writeText(ddlValue)}
                      >
                        {t('common.copyDdl')}
                      </Button>
                    </div>
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
                </div>
              ))}
              {preview.warnings.map((w) => (
                <p key={w} className="text-xs text-fg-muted">
                  {w}
                </p>
              ))}
            </div>
          )}

          {step === 'result' && result && (
            <div
              data-testid="data-transfer-result"
              className="space-y-3 rounded-lg border border-edge bg-surface-alt p-6 text-sm"
            >
              <p className="text-base font-medium">
                {t('transfer.rowsInserted')}: {result.rowsInserted}
              </p>
              {result.tables.map((tbl) => (
                <div key={tbl.sourceTable} className="rounded-lg border border-edge bg-surface p-3">
                  <div className="font-medium">
                    {tbl.sourceTable}: {tbl.success ? t('transfer.success') : t('transfer.error')}
                  </div>
                  {!tbl.success && tbl.error ? (
                    <CopyableError
                      message={tbl.error}
                      className="error-message mt-2 text-xs"
                      copyButton
                    />
                  ) : null}
                </div>
              ))}
            </div>
          )}
        </div>
      </div>

      <div className="flex shrink-0 items-center justify-between border-t border-edge px-6 py-3">
        <Button variant="ghost" disabled={stepIndex === 0 || executing} onClick={goBack}>
          <ChevronLeft className="h-4 w-4" /> {t('transfer.back')}
        </Button>
        <div className="flex items-center gap-2">
          {step === 'preview' && executing && (
            <Button
              variant="ghost"
              data-testid="data-transfer-cancel"
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
          {step === 'preview' ? (
            <Button
              variant="run"
              data-testid="data-transfer-execute"
              disabled={!canExecute || executing}
              onClick={handleExecuteClick}
            >
              {executing ? <Loader2 className="h-4 w-4 animate-spin" /> : null}
              {executing ? t('transfer.executing') : t('transfer.execute')}
            </Button>
          ) : step !== 'result' ? (
            <Button
              data-testid="data-transfer-next"
              disabled={!canNext || loading}
              onClick={() => void goNext()}
            >
              {loading ? <Loader2 className="h-4 w-4 animate-spin" /> : t('transfer.next')}
              <ChevronRight className="h-4 w-4" />
            </Button>
          ) : null}
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
