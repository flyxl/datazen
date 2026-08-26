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
import { databaseCommands } from '../../commands/database';
import { useI18n } from '../../hooks/useI18n';
import { useSettings } from '../../hooks/useSettings';
import { useSettingsStore } from '../../stores/settingsStore';

import { isTransferTargetSupported, resolveTransferPairing } from '../../lib/transferPairing';
import type { ConnectionConfig } from '../../types';
import { TransferMappingStep } from './TransferMappingStep';
import { normalizeColumnMappings, tableHasActiveMappings } from './transferMappingView';

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
  const [activeConns, setActiveConns] = useState<Record<string, string>>({});
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

  const ensureConnected = useCallback(
    async (connectionId: string): Promise<string | null> => {
      // activeConns maps persistent connection id -> live db session id.
      if (activeConns[connectionId]) return activeConns[connectionId];
      try {
        const dbSessionId = await invoke<string>('connect', { connectionId });
        setActiveConns((prev) => ({ ...prev, [connectionId]: dbSessionId }));
        return dbSessionId;
      } catch (e) {
        setErrorMsg(`${t('transfer.connectFailed')} ${e instanceof Error ? e.message : String(e)}`);
        setErrorOpen(true);
        return null;
      }
    },
    [activeConns, t],
  );

  useEffect(() => {
    if (!sourceId) return;
    let cancelled = false;
    const cfg = connections.find((c) => c.id === sourceId);
    (async () => {
      const connId = await ensureConnected(sourceId);
      if (!connId || cancelled) return;
      try {
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
  }, [sourceId, connections, ensureConnected]);

  useEffect(() => {
    if (!targetId) return;
    let cancelled = false;
    const cfg = connections.find((c) => c.id === targetId);
    (async () => {
      const connId = await ensureConnected(targetId);
      if (!connId || cancelled) return;
      try {
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
  }, [targetId, connections, ensureConnected]);

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
        })),
    [tables],
  );

  const buildJob = useCallback((): TransferJob | null => {
    const srcConnId = activeConns[sourceId];
    const tgtConnId = activeConns[targetId];
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
  }, [
    activeConns,
    sourceId,
    targetId,
    sourceDatabase,
    targetDatabase,
    mode,
    writeMode,
    tables,
    tablesToMappings,
    batchSize,
    stopOnError,
    confirmedDestructive,
  ]);

  const runInspect = useCallback(async () => {
    const srcConnId = await ensureConnected(sourceId);
    const tgtConnId = await ensureConnected(targetId);
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
  }, [ensureConnected, sourceId, targetId, sourceDatabase, targetDatabase, mode, t]);

  const runPreview = useCallback(async () => {
    const job = buildJob();
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
  }, [buildJob, t]);

  const runExecute = useCallback(async () => {
    const job = buildJob();
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
  }, [buildJob, targetReadOnly, t]);

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
    if (step === 'objects' && tables.length === 0) {
      await runInspect();
    }
    if (step === 'preview') {
      setStep('execute');
      return;
    }
    if (step === 'options') {
      await runPreview();
      setStep('preview');
      return;
    }
    if (step === 'mapping' && tables.length === 0) {
      await runInspect();
    }
    const next = STEPS[stepIndex + 1];
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
      const srcConnId = activeConns[sourceId];
      const tgtConnId = activeConns[targetId];
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
    [activeConns, sourceId, targetId, sourceDatabase, targetDatabase, mode, tablesToMappings],
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
            {preview.blockReason && <p className="text-warning">{preview.blockReason}</p>}
            {preview.ddl.map((item) => (
              <pre key={item.sourceTable} className="overflow-auto rounded bg-bg-muted p-2 text-xs">
                {item.ddl}
              </pre>
            ))}
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
              <div key={tbl.sourceTable}>
                {tbl.sourceTable}: {tbl.success ? t('transfer.success') : tbl.error}
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

      <Dialog open={errorOpen} onClose={() => setErrorOpen(false)} title={t('transfer.error')}>
        <p data-testid="data-transfer-error">{errorMsg}</p>
      </Dialog>
    </div>
  );
}
