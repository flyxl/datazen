import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { ChevronLeft, ChevronRight, Loader2 } from 'lucide-react';
import { TitleBar } from '../../components/TitleBar';
import { StatusBar } from '../../components/StatusBar';
import { Button } from '../../components/ui/Button';
import { Dialog } from '../../components/ui/Dialog';
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
import { useSettings } from '../../hooks/useSettings';
import { useSettingsStore } from '../../stores/settingsStore';

import { isTransferTargetSupported, resolveTransferPairing } from '../../lib/transferPairing';
import type { ConnectionConfig } from '../../types';
import { TransferMappingStep } from './TransferMappingStep';
import { normalizeColumnMappings, tableHasActiveMappings } from './transferMappingView';
import { SqlCodeBlock } from '../../components/SqlCodeBlock';
import {
  ensureDedicatedSession,
  listDatabasesDedicated,
  releaseDedicatedSession,
  type DedicatedSideSession,
} from '../../lib/dedicatedDbSession';

type WizardStep =
  | 'endpoints'
  | 'mode'
  | 'objects'
  | 'mapping'
  | 'options'
  | 'preview'
  | 'execute'
  | 'result';

const STEPS: WizardStep[] = [
  'endpoints',
  'mode',
  'objects',
  'mapping',
  'options',
  'preview',
  'execute',
  'result',
];

export function DataTransferWindow() {
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
  const [errorMsg, setErrorMsg] = useState('');
  const [errorOpen, setErrorOpen] = useState(false);
  const [selectedMappingTable, setSelectedMappingTable] = useState('');
  const jobIdRef = useRef<string | null>(null);

  useEffect(() => {
    void loadSettings();
  }, [loadSettings]);

  const loadConnections = useCallback(() => {
    void invoke<ConnectionConfig[]>('get_connections')
      .then(setConnections)
      .catch((e) => console.error('Failed to load connections', e));
  }, []);

  useEffect(() => {
    loadConnections();
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
      prev.map((t) => (t.sourceTable === sourceTable ? { ...t, ddlOverride: ddl } : t)),
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
          prev && enabled.some((t) => t.sourceTable === prev)
            ? prev
            : (enabled.find((t) => t.enabled)?.sourceTable ?? enabled[0].sourceTable),
        );
      }
    } catch (e) {
      setErrorMsg(e instanceof Error ? e.message : String(e));
      setErrorOpen(true);
    } finally {
      setLoading(false);
    }
  }, [refreshEndpointSessions, sourceDatabase, targetDatabase, mode, t]);

  const runPreview = useCallback(async () => {
    const sessions = await refreshEndpointSessions();
    const job = buildJob(sessions);
    if (!job) {
      setErrorMsg(t('transfer.selectBoth'));
      setErrorOpen(true);
      return;
    }
    setLoading(true);
    try {
      const p = await transferCommands.preview(job);
      setPreview(p);
    } catch (e) {
      setErrorMsg(e instanceof Error ? e.message : String(e));
      setErrorOpen(true);
    } finally {
      setLoading(false);
    }
  }, [refreshEndpointSessions, buildJob, t]);

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
    try {
      const execResult = await transferCommands.execute(job, jobId);
      setResult(execResult);
      setStep('result');
    } catch (e) {
      setErrorMsg(e instanceof Error ? e.message : String(e));
      setErrorOpen(true);
    } finally {
      setExecuting(false);
      jobIdRef.current = null;
    }
  }, [refreshEndpointSessions, buildJob, targetReadOnly, t]);

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
      case 'mode':
        return true;
      case 'objects':
        return tables.length === 0 || tables.some((tbl) => tbl.enabled);
      case 'mapping':
        return tables.some((tbl) => tbl.enabled && tableHasActiveMappings(tbl));
      case 'options':
        if (writeMode !== 'insert' && !confirmedDestructive) return false;
        return true;
      case 'preview':
        return preview?.canExecute === true && !targetReadOnly;
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
    preview,
    targetReadOnly,
  ]);

  const goNext = useCallback(async () => {
    if (step === 'preview') {
      setStep('execute');
      return;
    }
    if (step === 'options') {
      await runPreview();
      setStep('preview');
      return;
    }

    const next = STEPS[stepIndex + 1];
    if ((next === 'objects' || next === 'mapping') && tables.length === 0) {
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
      prev.map((tbl) => (tbl.sourceTable === sourceTable ? { ...tbl, ...patch } : tbl)),
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

  return (
    <div data-testid="data-transfer-window" className="flex h-screen flex-col bg-bg text-fg">
      <TitleBar title={t('common.dataTransfer')} />
      <div className="border-b border-border px-4 py-2">
        <div className="flex flex-wrap gap-2 text-xs text-fg-muted">
          {STEPS.map((s, i) => (
            <span
              key={s}
              data-testid={`data-transfer-step-${s}`}
              className={i === stepIndex ? 'font-semibold text-accent' : ''}
            >
              {i + 1}. {t(`transfer.step.${s}`)}
            </span>
          ))}
        </div>
      </div>

      <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-auto p-4">
        {step === 'endpoints' && (
          <div className="grid gap-4 md:grid-cols-2">
            <div data-testid="data-transfer-source">
              <label className="mb-1 block text-sm">{t('transfer.source')}</label>
              <Select
                value={sourceId}
                onChange={setSourceId}
                options={[{ value: '', label: t('common.selectConnection') }, ...connOptions]}
              />
              {sourceId && (
                <div className="mt-2" data-testid="data-transfer-source-database">
                  <Select
                    value={sourceDatabase}
                    onChange={setSourceDatabase}
                    options={sourceDatabases.map((d) => ({ value: d, label: d }))}
                  />
                </div>
              )}
            </div>
            <div data-testid="data-transfer-target">
              <label className="mb-1 block text-sm">{t('transfer.target')}</label>
              <Select
                value={targetId}
                onChange={setTargetId}
                options={[{ value: '', label: t('common.selectConnection') }, ...targetOptions]}
              />
              {targetId && (
                <div className="mt-2" data-testid="data-transfer-target-database">
                  <Select
                    value={targetDatabase}
                    onChange={setTargetDatabase}
                    options={targetDatabases.map((d) => ({ value: d, label: d }))}
                  />
                </div>
              )}
              {targetReadOnly && (
                <p className="mt-1 text-xs text-warning">{t('transfer.readOnlyHint')}</p>
              )}
            </div>
            {pairing && (
              <p data-testid="data-transfer-path" className="text-xs text-fg-muted md:col-span-2">
                {pairing.supported
                  ? t(`transfer.path.${pairing.path}`)
                  : (pairing.reason ?? t('common.unsupportedPair'))}
              </p>
            )}
          </div>
        )}

        {step === 'mode' && (
          <div className="flex flex-col gap-3">
            <label className="flex items-center gap-2">
              <input
                type="radio"
                name="transfer-mode"
                checked={mode === 'structure'}
                onChange={() => setMode('structure')}
                data-testid="data-transfer-mode-structure"
              />
              {t('common.structureOnly')}
            </label>
            <label className="flex items-center gap-2">
              <input
                type="radio"
                name="transfer-mode"
                checked={mode === 'data'}
                onChange={() => setMode('data')}
                data-testid="data-transfer-mode-data"
              />
              {t('common.dataOnly')}
            </label>
            <label className="flex items-center gap-2">
              <input
                type="radio"
                name="transfer-mode"
                checked={mode === 'structureAndData'}
                onChange={() => setMode('structureAndData')}
                data-testid="data-transfer-mode-both"
              />
              {t('transfer.mode.both')}
            </label>
          </div>
        )}

        {step === 'objects' && (
          <div>
            {loading ? (
              <Loader2 className="h-5 w-5 animate-spin" />
            ) : (
              <ul className="divide-y divide-border rounded border border-border">
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
                    <span className="flex-1">{tbl.sourceTable}</span>
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

        {step === 'options' && (
          <div className="flex max-w-md flex-col gap-3">
            <label className="text-sm">
              {t('transfer.writeMode.label')}
              <Select
                value={writeMode}
                onChange={(v) => setWriteMode(v as WriteMode)}
                options={[
                  { value: 'insert', label: t('transfer.writeMode.insert') },
                  { value: 'truncateInsert', label: t('transfer.writeMode.truncateInsert') },
                  { value: 'dropCreateInsert', label: t('transfer.writeMode.dropCreate') },
                ]}
              />
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
            <label className="text-sm">
              {t('transfer.batchSize')}
              <input
                type="number"
                min={1}
                className="mt-1 w-full rounded border border-border bg-bg px-2 py-1"
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
        )}

        {step === 'preview' && preview && (
          <div data-testid="data-transfer-preview" className="space-y-3 text-sm">
            {preview.blockReason && (
              <p className="select-text text-warning">{preview.blockReason}</p>
            )}
            {preview.ddl.map((item) => {
              const table = tables.find((t) => t.sourceTable === item.sourceTable);
              const ddlValue = table?.ddlOverride ?? item.ddl;
              return (
                <div
                  key={item.sourceTable}
                  className="overflow-hidden rounded border border-border"
                >
                  <div className="flex items-center justify-between gap-2 border-b border-border px-3 py-1.5 text-xs text-fg-muted">
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
                    className="h-48 min-h-[12rem] bg-bg-muted"
                    data-testid={`data-transfer-ddl-editor-${item.sourceTable}`}
                  >
                    <SqlCodeBlock
                      code={ddlValue}
                      dialect={targetConn?.databaseType ?? 'mysql'}
                      onChange={(next) => updateTableDdlOverride(item.sourceTable, next)}
                    />
                  </div>
                  <p className="border-t border-border px-3 py-1.5 text-[11px] text-fg-muted">
                    {t('transfer.ddlOverrideHint')}
                  </p>
                </div>
              );
            })}
            {preview.writePlans.map((plan) => (
              <div key={plan.sourceTable} className="rounded border border-border p-2">
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

        {step === 'execute' && (
          <div className="flex flex-col items-start gap-3">
            <p className="text-sm">{t('transfer.executePrompt')}</p>
            <Button
              data-testid="data-transfer-execute"
              disabled={executing || targetReadOnly}
              onClick={() => void runExecute()}
            >
              {executing ? <Loader2 className="h-4 w-4 animate-spin" /> : t('transfer.execute')}
            </Button>
            {executing && (
              <Button
                variant="ghost"
                data-testid="data-transfer-cancel"
                onClick={() => void handleCancel()}
              >
                {t('transfer.cancel')}
              </Button>
            )}
          </div>
        )}

        {step === 'result' && result && (
          <div data-testid="data-transfer-result" className="space-y-2 text-sm">
            <p>
              {t('transfer.rowsInserted')}: {result.rowsInserted}
            </p>
            {result.tables.map((tbl) => (
              <div key={tbl.sourceTable} className="rounded border border-border p-2">
                <div className="font-medium">
                  {tbl.sourceTable}: {tbl.success ? t('transfer.success') : t('transfer.error')}
                </div>
                {!tbl.success && tbl.error ? (
                  <div className="mt-2 flex items-start justify-between gap-2">
                    <pre className="min-w-0 flex-1 select-text whitespace-pre-wrap break-all text-xs text-danger">
                      {tbl.error}
                    </pre>
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => void navigator.clipboard.writeText(tbl.error ?? '')}
                    >
                      {t('common.copy')}
                    </Button>
                  </div>
                ) : null}
              </div>
            ))}
          </div>
        )}
      </div>

      <div className="flex items-center justify-between border-t border-border px-4 py-2">
        <Button variant="ghost" disabled={stepIndex === 0} onClick={goBack}>
          <ChevronLeft className="h-4 w-4" /> {t('transfer.back')}
        </Button>
        {step !== 'result' && step !== 'execute' && (
          <Button
            data-testid="data-transfer-next"
            disabled={!canNext || loading}
            onClick={() => void goNext()}
          >
            {loading ? <Loader2 className="h-4 w-4 animate-spin" /> : t('transfer.next')}
            <ChevronRight className="h-4 w-4" />
          </Button>
        )}
      </div>

      <StatusBar />

      <Dialog
        open={errorOpen}
        onClose={() => setErrorOpen(false)}
        title={t('transfer.error')}
        footer={
          <Button
            variant="ghost"
            data-testid="data-transfer-copy-error"
            onClick={() => void navigator.clipboard.writeText(errorMsg)}
          >
            {t('common.copy')}
          </Button>
        }
      >
        <pre
          data-testid="data-transfer-error"
          className="max-h-64 select-text overflow-auto whitespace-pre-wrap break-all text-sm"
        >
          {errorMsg}
        </pre>
      </Dialog>
    </div>
  );
}
