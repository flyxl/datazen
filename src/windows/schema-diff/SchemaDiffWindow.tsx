import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { BookOpen, ChevronLeft, ChevronRight, Copy, Loader2 } from 'lucide-react';
import { TitleBar } from '../../components/TitleBar';
import { StatusBar } from '../../components/StatusBar';
import { LocaleDomainLoading } from '../../components/LocaleDomainLoading';
import { Button } from '../../components/ui/Button';
import { Dialog } from '../../components/ui/Dialog';
import { CopyableError } from '../../components/ui/CopyableError';
import { SchemaDiffPanel, formatSchemaDiffText } from '../../components/schema/SchemaDiffPanel';
import {
  dialectSupportsTransactionalDdl,
  exportPlanSql,
  planHasDestructive,
  schemaDiffCommands,
  type SchemaDiffConfigJson,
  type SchemaDiffDeployResult,
  type SchemaDiffPlan,
} from '../../commands/schemaDiff';
import { databaseCommands } from '../../commands/database';
import { fileCommands } from '../../commands/file';
import { useSettings } from '../../hooks/useSettings';
import { useI18n } from '../../hooks/useI18n';
import { useLocaleDomains } from '../../hooks/useLocaleDomains';
import { useResizable } from '../../hooks/useResizable';
import { useSettingsStore } from '../../stores/settingsStore';
import { openDocsWindow } from '../../lib/windowManager';
import { cn } from '../../lib/cn';
import { MigrationEndpointsBar } from '../../components/migration/MigrationEndpointsBar';
import type { TableSchemaDiff } from '../../types';
import {
  isSchemaDiffLimitationsDismissed,
  setSchemaDiffLimitationsDismissed,
} from '../../lib/schemaDiffLimitationsPrefs';
import { LimitationsDialog } from '../../components/ui/LimitationsDialog';
import { SCHEMA_DIFF_LIMITATION_KEYS } from './schemaDiffLimitationKeys';
import { SchemaDiffTableListPanel } from './SchemaDiffTableListPanel';
import { SchemaDiffRightPanel } from './SchemaDiffRightPanel';
import { SchemaDiffObjectsStep } from './SchemaDiffObjectsStep';
import { useSchemaDiffEndpoints } from './useSchemaDiffEndpoints';
import {
  enabledTableNames,
  filterTablesForSchema,
  qualifySchemaDiffTableName,
  type SchemaDiffTablePick,
} from './schemaDiffTableNames';

type WizardStep = 'endpoints' | 'objects' | 'compare' | 'plan' | 'deploy';

type ClipboardFeedback = 'summary' | 'sql' | 'config' | null;

const STEPS: WizardStep[] = ['endpoints', 'objects', 'compare', 'plan', 'deploy'];
const NARROW_STEPS: WizardStep[] = ['endpoints', 'objects', 'deploy'];

function tableDiffHasChanges(diff: TableSchemaDiff): boolean {
  const missing = diff.missingOnTarget ?? diff.added;
  const extra = diff.extraOnTarget ?? diff.removed;
  return missing.length > 0 || extra.length > 0 || diff.changed.length > 0;
}

export function SchemaDiffWindow() {
  const localesReady = useLocaleDomains(['schemaDiff']);
  useSettings();
  const { t } = useI18n();
  const loadSettings = useSettingsStore((s) => s.loadSettings);

  const [step, setStep] = useState<WizardStep>('endpoints');
  const [tablePicks, setTablePicks] = useState<SchemaDiffTablePick[]>([]);
  const [objectsLoading, setObjectsLoading] = useState(false);
  const [diffs, setDiffs] = useState<TableSchemaDiff[]>([]);
  const [plan, setPlan] = useState<SchemaDiffPlan | null>(null);
  const [selectedTable, setSelectedTable] = useState<string | null>(null);
  const [allowDestructive, setAllowDestructive] = useState(false);
  const [includeIndexes, setIncludeIndexes] = useState(true);
  const [requireRollback, setRequireRollback] = useState(false);
  const [useTransaction, setUseTransaction] = useState(true);
  const [confirmText, setConfirmText] = useState('');
  const [deployResult, setDeployResult] = useState<SchemaDiffDeployResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');
  const [clipboardFeedback, setClipboardFeedback] = useState<ClipboardFeedback>(null);
  const [importConfigOpen, setImportConfigOpen] = useState(false);
  const [importConfigText, setImportConfigText] = useState('');
  const [importConfigError, setImportConfigError] = useState('');
  const [limitationsOpen, setLimitationsOpen] = useState(false);
  const planAutoRequestedRef = useRef(false);

  const { size: tableListWidth, handleRef: tableListResizeRef } = useResizable({
    direction: 'horizontal',
    initialSize: 200,
    minSize: 120,
    maxSize: 400,
    storageKey: 'schema-diff.table-list',
  });

  const endpoints = useSchemaDiffEndpoints({ onError: setError });

  const selectedTables = useMemo(() => enabledTableNames(tablePicks), [tablePicks]);

  useEffect(() => {
    void loadSettings();
  }, [loadSettings]);

  useEffect(() => {
    if (!isSchemaDiffLimitationsDismissed()) {
      setLimitationsOpen(true);
    }
  }, []);

  useEffect(() => {
    setTablePicks([]);
    setDiffs([]);
    setPlan(null);
    setDeployResult(null);
    setSelectedTable(null);
    planAutoRequestedRef.current = false;
    // eslint-disable-next-line react-hooks/exhaustive-deps -- clear compare artifacts when endpoints change
  }, [
    endpoints.sourceId,
    endpoints.targetId,
    endpoints.sourceDatabase,
    endpoints.targetDatabase,
    endpoints.sourceSchema,
    endpoints.targetSchema,
  ]);

  const showClipboardFeedback = useCallback((kind: ClipboardFeedback) => {
    setClipboardFeedback(kind);
    window.setTimeout(() => setClipboardFeedback(null), 2000);
  }, []);

  useEffect(() => {
    if (selectedTables.length === 0) {
      setSelectedTable(null);
      return;
    }
    setSelectedTable((prev) =>
      prev && selectedTables.includes(prev) ? prev : (selectedTables[0] ?? null),
    );
  }, [selectedTables]);

  const tableHasDiff = useMemo(() => {
    const map: Record<string, boolean> = {};
    for (const diff of diffs) {
      map[diff.table] = tableDiffHasChanges(diff);
    }
    return map;
  }, [diffs]);

  const selectedDiff = useMemo(
    () => diffs.find((d) => d.table === selectedTable) ?? null,
    [diffs, selectedTable],
  );

  const targetLabel = useMemo(() => {
    const c = endpoints.targetConn;
    return c ? `${c.name} (${c.databaseType})` : endpoints.targetId;
  }, [endpoints.targetConn, endpoints.targetId]);

  const stepIndex = STEPS.indexOf(step);

  const loadSourceTables = useCallback(async () => {
    if (!endpoints.validateEndpoints()) return;
    setObjectsLoading(true);
    setError('');
    try {
      const srcConnId = await endpoints.ensureConnected('source');
      if (!srcConnId) return;
      const rows = await databaseCommands.getTables(srcConnId, endpoints.sourceDatabase);
      const filtered = filterTablesForSchema(rows, endpoints.sourceSchema || undefined);
      const picks = filtered.map((table) => ({
        name: qualifySchemaDiffTableName(table, endpoints.sourceSchema || undefined),
        enabled: true,
      }));
      picks.sort((a, b) => a.name.localeCompare(b.name));
      setTablePicks(picks);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setTablePicks([]);
    } finally {
      setObjectsLoading(false);
    }
  }, [endpoints]);

  const runCompare = useCallback(async (): Promise<boolean> => {
    setError('');
    setDiffs([]);
    setPlan(null);
    setDeployResult(null);
    if (!endpoints.validateEndpoints()) return false;

    const tables = enabledTableNames(tablePicks);
    if (tables.length === 0) {
      setError(t('schemaDiff.tableRequired'));
      return false;
    }

    setLoading(true);
    try {
      const srcConnId = await endpoints.ensureConnected('source');
      const tgtConnId = await endpoints.ensureConnected('target');
      if (!srcConnId || !tgtConnId) return false;
      const results: TableSchemaDiff[] = [];
      for (const table of tables) {
        results.push(await schemaDiffCommands.compareTableSchemas(srcConnId, tgtConnId, table));
      }
      setDiffs(results);
      setSelectedTable(tables[0] ?? null);
      return true;
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      return false;
    } finally {
      setLoading(false);
    }
  }, [endpoints, tablePicks, t]);

  const buildPlan = useCallback(async () => {
    setError('');
    setDeployResult(null);
    const tables = enabledTableNames(tablePicks);
    if (tables.length === 0) {
      setError(t('schemaDiff.tableRequired'));
      return;
    }
    if (!endpoints.validateEndpoints()) return;

    setLoading(true);
    try {
      const srcConnId = await endpoints.ensureConnected('source');
      const tgtConnId = await endpoints.ensureConnected('target');
      if (!srcConnId || !tgtConnId) return;
      const next = await schemaDiffCommands.preparePlan({
        sourceDbSessionId: srcConnId,
        targetDbSessionId: tgtConnId,
        tableNames: tables,
        allowDestructive,
        includeIndexes,
      });
      setPlan(next);
      setUseTransaction(dialectSupportsTransactionalDdl(next.targetDialect));
      setConfirmText('');
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, [allowDestructive, endpoints, includeIndexes, tablePicks, t]);

  useEffect(() => {
    if (step !== 'plan') {
      planAutoRequestedRef.current = false;
      return;
    }
    if (plan || loading || planAutoRequestedRef.current) return;
    planAutoRequestedRef.current = true;
    void buildPlan();
  }, [step, plan, loading, buildPlan]);

  const handleDeploy = useCallback(async () => {
    if (!plan) return;
    setError('');
    setLoading(true);
    try {
      const tgtConnId = await endpoints.ensureConnected('target');
      if (!tgtConnId) return;
      const result = await schemaDiffCommands.executeDeploy({
        targetDbSessionId: tgtConnId,
        plan,
        useTransaction,
        confirmDestructive: planHasDestructive(plan) ? confirmText.trim() : undefined,
      });
      setDeployResult(result);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, [confirmText, endpoints, plan, useTransaction]);

  const canNext = useMemo(() => {
    switch (step) {
      case 'endpoints':
        return Boolean(
          endpoints.sourceId &&
            endpoints.targetId &&
            endpoints.sourceDatabase &&
            endpoints.targetDatabase &&
            !endpoints.isSameEndpoint(),
        );
      case 'objects':
        return !objectsLoading;
      case 'compare':
        return diffs.length > 0 && !loading;
      case 'plan':
        return plan !== null && !loading;
      default:
        return false;
    }
  }, [
    step,
    endpoints.sourceId,
    endpoints.targetId,
    endpoints.sourceDatabase,
    endpoints.targetDatabase,
    endpoints.isSameEndpoint,
    selectedTables.length,
    objectsLoading,
    diffs.length,
    loading,
    plan,
  ]);

  const goNext = useCallback(async () => {
    const next = STEPS[stepIndex + 1];
    if (step === 'objects' && next === 'compare') {
      const ok = await runCompare();
      if (ok) setStep('compare');
      return;
    }
    if (step === 'plan' && next === 'deploy') {
      setStep('deploy');
      return;
    }
    if (next === 'objects' && tablePicks.length === 0) {
      await loadSourceTables();
    }
    if (next) setStep(next);
  }, [step, stepIndex, runCompare, tablePicks.length, loadSourceTables]);

  const goBack = () => {
    const prev = STEPS[stepIndex - 1];
    if (prev) setStep(prev);
  };

  const toggleTable = (name: string) => {
    setTablePicks((prev) =>
      prev.map((row) => (row.name === name ? { ...row, enabled: !row.enabled } : row)),
    );
  };

  const handleCopySummary = async () => {
    if (diffs.length === 0) return;
    try {
      await navigator.clipboard.writeText(diffs.map(formatSchemaDiffText).join('\n\n'));
      showClipboardFeedback('summary');
    } catch {
      setError(t('schemaDiff.clipboardFailed'));
    }
  };

  const handleCopySql = async () => {
    if (!plan) return;
    try {
      await navigator.clipboard.writeText(exportPlanSql(plan));
      showClipboardFeedback('sql');
    } catch {
      setError(t('schemaDiff.clipboardFailed'));
    }
  };

  const handleExportConfig = async () => {
    const cfg: SchemaDiffConfigJson = {
      version: 2,
      sourceConnectionId: endpoints.sourceId,
      targetConnectionId: endpoints.targetId,
      tables: selectedTables,
      allowDestructive,
      includeIndexes,
      requireRollback,
    };
    try {
      const saved = await fileCommands.saveTextWithDialog(
        JSON.stringify(cfg, null, 2),
        'schema-diff-config.json',
        'JSON',
        ['json'],
      );
      if (saved) {
        showClipboardFeedback('config');
      }
    } catch {
      setError(t('schemaDiff.exportConfigFailed'));
    }
  };

  const applyImportedConfig = useCallback(
    (text: string) => {
      setImportConfigError('');
      try {
        const cfg = JSON.parse(text) as SchemaDiffConfigJson;
        if (cfg.version !== 2 || !cfg.sourceConnectionId || !cfg.targetConnectionId) {
          throw new Error(t('schemaDiff.invalidConfig'));
        }
        endpoints.setSourceId(cfg.sourceConnectionId);
        endpoints.setTargetId(cfg.targetConnectionId);
        setTablePicks((cfg.tables ?? []).map((name) => ({ name, enabled: true })));
        setAllowDestructive(Boolean(cfg.allowDestructive));
        setIncludeIndexes(cfg.includeIndexes ?? true);
        setRequireRollback(Boolean(cfg.requireRollback));
        setPlan(null);
        setDiffs([]);
        setDeployResult(null);
        planAutoRequestedRef.current = false;
        setStep('objects');
        setImportConfigOpen(false);
        setImportConfigText('');
        setImportConfigError('');
      } catch (e) {
        setImportConfigError(e instanceof Error ? e.message : String(e));
      }
    },
    [endpoints, t],
  );

  const handleOpenImportConfig = () => {
    setError('');
    setImportConfigText('');
    setImportConfigError('');
    setImportConfigOpen(true);
  };

  const endpointsCrossDialectNote = endpoints.isCrossDialect ? (
    <span
      data-testid="schema-diff-cross-dialect-note"
      className="mt-4 inline-block max-w-full rounded border border-edge bg-surface px-2 py-1 text-xs text-fg-muted"
    >
      {t('schemaDiff.crossDialectNote')}
    </span>
  ) : undefined;

  // All hooks above. Gate the body on the `schemaDiff` locale pack so the UI
  // never renders raw/un-translated `t('schemaDiff.*')` keys before it loads.
  if (!localesReady) {
    return <LocaleDomainLoading testId="schema-diff-locale-loading" />;
  }

  return (
    <div
      className="flex h-screen min-h-0 flex-col bg-surface text-fg"
      data-testid="schema-diff-window"
    >
      <TitleBar
        title={t('common.schemaDiff')}
        rightContent={
          <Button
            variant="secondary"
            className="h-6 w-6 !px-0"
            title={t('docs.openSchemaDiffHelp')}
            onClick={() => openDocsWindow('schemaDiff')}
          >
            <BookOpen className="h-3 w-3" />
          </Button>
        }
      />

      <div className="border-b border-edge px-6 py-3">
        <div className="mx-auto flex max-w-4xl flex-wrap items-center justify-center gap-1">
          {STEPS.map((s, i) => (
            <div key={s} className="flex items-center gap-1">
              {i > 0 && <ChevronRight className="h-3 w-3 shrink-0 text-fg-muted" aria-hidden />}
              <span
                data-testid={`schema-diff-step-${s}`}
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
                {t(`schemaDiff.step.${s}`)}
              </span>
            </div>
          ))}
        </div>
      </div>

      <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
        <div
          className={cn(
            'mx-auto flex min-h-0 w-full flex-1 flex-col px-6 py-6',
            NARROW_STEPS.includes(step) ? 'max-w-2xl overflow-auto' : 'max-w-6xl',
          )}
        >
          {step === 'endpoints' && (
            <MigrationEndpointsBar
              layout="grid"
              testIdPrefix="schema-diff"
              showSwap={false}
              showCompare={false}
              sourceId={endpoints.sourceId}
              targetId={endpoints.targetId}
              sourceDatabase={endpoints.sourceDatabase}
              targetDatabase={endpoints.targetDatabase}
              sourceSchema={endpoints.sourceSchema}
              targetSchema={endpoints.targetSchema}
              sourceDatabases={endpoints.sourceDatabases}
              targetDatabases={endpoints.targetDatabases}
              sourceSchemas={endpoints.sourceSchemas}
              targetSchemas={endpoints.targetSchemas}
              connOptions={endpoints.connOptions}
              targetOptions={endpoints.targetOptions}
              footerNote={endpointsCrossDialectNote}
              onSourceChange={endpoints.setSourceId}
              onTargetChange={endpoints.setTargetId}
              onSourceDatabaseChange={endpoints.setSourceDatabase}
              onTargetDatabaseChange={endpoints.setTargetDatabase}
              onSourceSchemaChange={endpoints.setSourceSchema}
              onTargetSchemaChange={endpoints.setTargetSchema}
            />
          )}

          {step === 'objects' && (
            <SchemaDiffObjectsStep
              loading={objectsLoading}
              tables={tablePicks}
              onToggle={toggleTable}
              onSelectAll={() =>
                setTablePicks((prev) => prev.map((row) => ({ ...row, enabled: true })))
              }
              onSelectNone={() =>
                setTablePicks((prev) => prev.map((row) => ({ ...row, enabled: false })))
              }
            />
          )}

          {step === 'compare' && (
            <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
              {loading && diffs.length === 0 ? (
                <div className="flex flex-1 items-center justify-center gap-2 text-sm text-fg-muted">
                  <Loader2 className="h-4 w-4 animate-spin" />
                  {t('schemaDiff.compare')}
                </div>
              ) : (
                <div className="flex min-h-0 flex-1 overflow-hidden rounded-lg border border-edge">
                  <SchemaDiffTableListPanel
                    className="flex-none"
                    style={{ width: tableListWidth }}
                    tables={selectedTables}
                    selectedTable={selectedTable}
                    onSelect={setSelectedTable}
                    tableHasDiff={diffs.length > 0 ? tableHasDiff : undefined}
                  />
                  <div
                    ref={tableListResizeRef}
                    className="w-1 shrink-0 cursor-col-resize bg-transparent hover:bg-accent/30"
                  />
                  <div
                    className="flex min-h-0 min-w-0 flex-1 flex-col overflow-auto bg-surface-alt/30"
                    data-testid="schema-diff-detail-panel"
                  >
                    {selectedDiff ? (
                      <div className="p-4">
                        <div className="mb-3 flex items-center justify-between gap-2">
                          <h3 className="font-mono text-sm font-medium text-fg">
                            {selectedDiff.table}
                          </h3>
                          {diffs.length > 0 && (
                            <Button
                              variant="secondary"
                              size="sm"
                              onClick={() => void handleCopySummary()}
                            >
                              <Copy className="h-4 w-4" />
                              {clipboardFeedback === 'summary'
                                ? t('common.copied')
                                : t('schemaDiff.copySummary')}
                            </Button>
                          )}
                        </div>
                        <SchemaDiffPanel diff={selectedDiff} />
                      </div>
                    ) : (
                      <div className="flex flex-1 items-center justify-center p-4 text-sm text-fg-muted">
                        {t('schemaDiff.selectTableHint')}
                      </div>
                    )}
                  </div>
                </div>
              )}
            </div>
          )}

          {step === 'plan' && (
            <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-hidden">
              {loading && !plan && (
                <div className="flex items-center gap-2 text-sm text-fg-muted">
                  <Loader2 className="h-4 w-4 animate-spin" />
                  {t('schemaDiff.generating')}
                </div>
              )}
              <div className="flex flex-wrap items-center gap-2">
                {plan && (
                  <Button
                    variant="secondary"
                    data-testid="schema-diff-copy-sql"
                    onClick={() => void handleCopySql()}
                  >
                    <Copy className="h-4 w-4" />
                    {clipboardFeedback === 'sql' ? t('common.copied') : t('common.copySql')}
                  </Button>
                )}
                <Button
                  variant="ghost"
                  data-testid="schema-diff-export-config"
                  onClick={() => void handleExportConfig()}
                >
                  {clipboardFeedback === 'config'
                    ? t('schemaDiff.configExported')
                    : t('schemaDiff.exportConfig')}
                </Button>
                <Button
                  variant="ghost"
                  data-testid="schema-diff-import-config"
                  onClick={handleOpenImportConfig}
                >
                  {t('schemaDiff.importConfig')}
                </Button>
              </div>
              <SchemaDiffRightPanel
                className="min-h-0 flex-1 border border-edge"
                activeTab="plan"
                onTabChange={() => {}}
                plan={plan}
                allowDestructive={allowDestructive}
                includeIndexes={includeIndexes}
                onAllowDestructiveChange={setAllowDestructive}
                onIncludeIndexesChange={setIncludeIndexes}
                onRegenerate={() => void buildPlan()}
                regenerating={loading && Boolean(plan)}
                targetLabel={targetLabel}
                useTransaction={useTransaction}
                onUseTransactionChange={setUseTransaction}
                requireRollback={requireRollback}
                onRequireRollbackChange={setRequireRollback}
                confirmText={confirmText}
                onConfirmTextChange={setConfirmText}
                deploying={false}
                onDeploy={() => {}}
                deployResult={null}
                hideTabs
              />
            </div>
          )}

          {step === 'deploy' && (
            <SchemaDiffRightPanel
              className="min-h-0 flex-1 border border-edge"
              activeTab="deploy"
              onTabChange={() => {}}
              plan={plan}
              allowDestructive={allowDestructive}
              includeIndexes={includeIndexes}
              onAllowDestructiveChange={setAllowDestructive}
              onIncludeIndexesChange={setIncludeIndexes}
              onRegenerate={() => void buildPlan()}
              regenerating={loading && Boolean(plan)}
              targetLabel={targetLabel}
              useTransaction={useTransaction}
              onUseTransactionChange={setUseTransaction}
              requireRollback={requireRollback}
              onRequireRollbackChange={setRequireRollback}
              confirmText={confirmText}
              onConfirmTextChange={setConfirmText}
              deploying={loading}
              onDeploy={() => void handleDeploy()}
              deployResult={deployResult}
              hideTabs
              hideDeployButton
            />
          )}

          {error && <CopyableError message={error} className="error-message mt-3" />}
        </div>
      </div>

      <div className="flex shrink-0 items-center justify-between border-t border-edge px-6 py-3">
        <Button variant="ghost" disabled={stepIndex === 0 || loading} onClick={goBack}>
          <ChevronLeft className="h-4 w-4" /> {t('transfer.back')}
        </Button>
        <div className="flex items-center gap-2">
          {step === 'deploy' ? (
            <Button
              variant="run"
              data-testid="schema-diff-deploy"
              disabled={!plan || loading}
              onClick={() => void handleDeploy()}
            >
              {loading ? <Loader2 className="h-4 w-4 animate-spin" /> : t('schemaDiff.deploy')}
            </Button>
          ) : (
            <Button
              data-testid="schema-diff-next"
              disabled={!canNext || loading}
              onClick={() => void goNext()}
            >
              {loading ? <Loader2 className="h-4 w-4 animate-spin" /> : t('transfer.next')}
              <ChevronRight className="h-4 w-4" />
            </Button>
          )}
        </div>
      </div>

      <LimitationsDialog
        open={limitationsOpen}
        onClose={() => setLimitationsOpen(false)}
        titleKey="schemaDiff.limitations.title"
        dontShowAgainKey="schemaDiff.limitations.dontShowAgain"
        limitationKeys={SCHEMA_DIFF_LIMITATION_KEYS}
        testIdPrefix="schema-diff"
        onDismiss={setSchemaDiffLimitationsDismissed}
      />
      <Dialog
        open={importConfigOpen}
        title={t('schemaDiff.importConfigTitle')}
        description={t('schemaDiff.importConfigHint')}
        testId="schema-diff-import-config-dialog"
        onClose={() => {
          setImportConfigOpen(false);
          setImportConfigText('');
          setImportConfigError('');
        }}
        footer={
          <>
            <Button
              variant="ghost"
              onClick={() => {
                setImportConfigOpen(false);
                setImportConfigText('');
                setImportConfigError('');
              }}
            >
              {t('common.cancel')}
            </Button>
            <Button
              data-testid="schema-diff-import-config-confirm"
              disabled={!importConfigText.trim()}
              onClick={() => applyImportedConfig(importConfigText)}
            >
              {t('schemaDiff.importConfigConfirm')}
            </Button>
          </>
        }
      >
        <textarea
          data-testid="schema-diff-import-config-text"
          className="h-48 w-full resize-y rounded-md border border-edge bg-surface px-3 py-2 font-mono text-xs text-fg"
          value={importConfigText}
          placeholder={t('schemaDiff.importConfigPlaceholder')}
          onChange={(e) => {
            setImportConfigText(e.target.value);
            if (importConfigError) setImportConfigError('');
          }}
        />
        {importConfigError && (
          <CopyableError
            message={importConfigError}
            className="error-message mt-2"
            data-testid="schema-diff-import-config-error"
          />
        )}
      </Dialog>
      <StatusBar left={<span className="truncate">{t('common.schemaDiff')}</span>} />
    </div>
  );
}
