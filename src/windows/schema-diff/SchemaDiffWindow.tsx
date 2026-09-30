import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { BookOpen, ChevronLeft, ChevronRight, Copy } from 'lucide-react';
import { TitleBar } from '../../components/TitleBar';
import { StatusBar } from '../../components/StatusBar';
import { LocaleDomainLoading } from '../../components/LocaleDomainLoading';
import { Button } from '../../components/ui/Button';
import { Select } from '../../components/ui/Select';
import { CopyableError } from '../../components/ui/CopyableError';
import { formatSchemaDiffText } from '../../components/schema/SchemaDiffPanel';
import {
  dialectSupportsTransactionalDdl,
  exportPlanSql,
  planHasDestructive,
  schemaDiffCommands,
  type ColumnTypeOverride,
  type SchemaDiffDeployResult,
  type SchemaDiffObjectIdentity,
  type SchemaDiffPlan,
} from '../../commands/schemaDiff';
import { databaseCommands } from '../../commands/database';
import { useSettings } from '../../hooks/useSettings';
import { useI18n } from '../../hooks/useI18n';
import { useLocaleDomains } from '../../hooks/useLocaleDomains';
import { useResizable } from '../../hooks/useResizable';
import { useSettingsStore } from '../../stores/settingsStore';
import { openDocsWindow } from '../../lib/windowManager';
import { cn } from '../../lib/cn';
import { canRunDeploy } from '../../lib/schemaDiffConfirm';
import { MigrationRunHistoryDialog } from '../../components/migration/MigrationRunHistoryDialog';
import type { TableSchemaDiff } from '../../types';
import {
  isSchemaDiffLimitationsDismissed,
  setSchemaDiffLimitationsDismissed,
} from '../../lib/schemaDiffLimitationsPrefs';
import { LimitationsDialog } from '../../components/ui/LimitationsDialog';
import { Spinner } from '../../components/ui/Spinner';
import { SCHEMA_DIFF_LIMITATION_KEYS } from './schemaDiffLimitationKeys';
import { SchemaDiffExecutionPanels } from './SchemaDiffExecutionPanels';
import { SchemaDiffSelectionSteps } from './SchemaDiffSelectionSteps';
import { SchemaDiffSavedSetupDialogs } from './SchemaDiffSavedSetupDialogs';
import {
  SCHEMA_DIFF_NARROW_STEPS,
  SCHEMA_DIFF_STEPS,
  SchemaDiffWizardProgress,
  type SchemaDiffWizardStep,
} from './SchemaDiffWizardProgress';
import { useSchemaDiffUnifiedObjects } from './useSchemaDiffUnifiedObjects';
import { useSchemaDiffEndpoints } from './useSchemaDiffEndpoints';
import { useSchemaDiffSavedSetups } from './useSchemaDiffSavedSetups';
import {
  enabledTableNames,
  enabledSourceTableNames,
  enabledTargetOnlyTableNames,
  mergeSchemaDiffTablePicks,
  type SchemaDiffTablePick,
} from './schemaDiffTableNames';

type ClipboardFeedback = 'summary' | 'sql' | 'config' | null;

function tableDiffHasChanges(diff: TableSchemaDiff): boolean {
  if (diff.targetOnly) return true;
  const missing = diff.missingOnTarget ?? diff.added;
  const extra = diff.extraOnTarget ?? diff.removed;
  return (
    missing.length > 0 ||
    extra.length > 0 ||
    diff.changed.length > 0 ||
    (diff.missingCheckConstraints?.length ?? 0) > 0 ||
    (diff.extraCheckConstraints?.length ?? 0) > 0 ||
    Boolean(diff.tableOptions)
  );
}

export function SchemaDiffWindow() {
  const localesReady = useLocaleDomains(['sync']);
  useSettings();
  const { t } = useI18n();
  const loadSettings = useSettingsStore((s) => s.loadSettings);

  const [step, setStep] = useState<SchemaDiffWizardStep>('endpoints');
  const [tablePicks, setTablePicks] = useState<SchemaDiffTablePick[]>([]);
  const [objectsLoading, setObjectsLoading] = useState(false);
  const unifiedObjects = useSchemaDiffUnifiedObjects();
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
  const [limitationsOpen, setLimitationsOpen] = useState(false);
  const [typeOverrides, setTypeOverrides] = useState<ColumnTypeOverride[]>([]);
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

  const showClipboardFeedback = useCallback((kind: ClipboardFeedback) => {
    setClipboardFeedback(kind);
    window.setTimeout(() => setClipboardFeedback(null), 2000);
  }, []);

  const savedSetups = useSchemaDiffSavedSetups({
    endpoints,
    tablePicks,
    setTablePicks,
    selectedTables,
    unifiedObjects,
    objectsLoading,
    setObjectsLoading,
    allowDestructive,
    setAllowDestructive,
    includeIndexes,
    setIncludeIndexes,
    requireRollback,
    setRequireRollback,
    typeOverrides,
    setTypeOverrides,
    setStep,
    setPlan,
    setDiffs,
    setDeployResult,
    setError,
    planAutoRequestedRef,
    onConfigExported: () => showClipboardFeedback('config'),
  });

  useEffect(() => {
    void loadSettings();
  }, [loadSettings]);

  useEffect(() => {
    if (!isSchemaDiffLimitationsDismissed()) {
      setLimitationsOpen(true);
    }
  }, []);

  useEffect(() => {
    if (savedSetups.shouldPreserveEndpointChange()) return;
    setTablePicks([]);
    unifiedObjects.clear();
    setDiffs([]);
    setPlan(null);
    setDeployResult(null);
    setSelectedTable(null);
    setTypeOverrides([]);
    planAutoRequestedRef.current = false;
    // eslint-disable-next-line react-hooks/exhaustive-deps -- clear compare artifacts when endpoints change
  }, [
    endpoints.sourceId,
    endpoints.targetId,
    endpoints.sourceDatabase,
    endpoints.targetDatabase,
    endpoints.sourceSchema,
    endpoints.targetSchema,
    unifiedObjects.clear,
    savedSetups.shouldPreserveEndpointChange,
  ]);

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

  const stepIndex = SCHEMA_DIFF_STEPS.indexOf(step);
  const selectedSavedProfile = savedSetups.profiles.find(
    (profile) => profile.id === savedSetups.selectedProfileId,
  );

  const loadSourceTables = useCallback(async () => {
    if (!endpoints.validateEndpoints()) return;
    setObjectsLoading(true);
    setError('');
    try {
      const srcConnId = await endpoints.ensureConnected('source');
      const tgtConnId = await endpoints.ensureConnected('target');
      if (!srcConnId || !tgtConnId) return;
      const [sourceRows, targetRows] = await Promise.all([
        databaseCommands.getTables(srcConnId, endpoints.sourceDatabase),
        databaseCommands.getTables(tgtConnId, endpoints.targetDatabase),
      ]);
      const catalog = await unifiedObjects.load(
        srcConnId,
        tgtConnId,
        endpoints.sourceSchema,
        endpoints.targetSchema,
      );
      if (catalog) {
        unifiedObjects.restoreSelection(
          catalog,
          unifiedObjects.selectedSourceObjects,
          unifiedObjects.selectedTargetObjects,
        );
      }
      setTablePicks(
        mergeSchemaDiffTablePicks(
          sourceRows,
          targetRows,
          endpoints.sourceSchema || undefined,
          endpoints.targetSchema || undefined,
        ),
      );
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setTablePicks([]);
    } finally {
      setObjectsLoading(false);
    }
  }, [
    endpoints,
    unifiedObjects.load,
    unifiedObjects.restoreSelection,
    unifiedObjects.selectedSourceObjects,
    unifiedObjects.selectedTargetObjects,
  ]);

  const runCompare = useCallback(async (): Promise<boolean> => {
    setError('');
    setDiffs([]);
    setPlan(null);
    setDeployResult(null);
    if (!endpoints.validateEndpoints()) return false;

    const selected = tablePicks.filter((row) => row.enabled);
    if (selected.length === 0) {
      setError(t('schemaDiff.tableRequired'));
      return false;
    }

    setLoading(true);
    try {
      const srcConnId = await endpoints.ensureConnected('source');
      const tgtConnId = await endpoints.ensureConnected('target');
      if (!srcConnId || !tgtConnId) return false;
      const results: TableSchemaDiff[] = [];
      for (const pick of selected) {
        if (pick.origin === 'target-only') {
          results.push({
            table: pick.name,
            targetOnly: true,
            missingOnTarget: [],
            extraOnTarget: [],
            added: [],
            removed: [],
            changed: [],
          });
          continue;
        }
        const sourceTable = pick.sourceName ?? pick.name;
        const targetTable = pick.targetName ?? pick.name.split('.').at(-1) ?? pick.name;
        results.push(
          await schemaDiffCommands.compareTableSchemas(
            srcConnId,
            tgtConnId,
            sourceTable,
            targetTable,
            endpoints.sourceSchema || undefined,
            endpoints.targetSchema || undefined,
          ),
        );
      }
      setDiffs(results);
      setSelectedTable(selected[0]?.name ?? null);
      return true;
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      return false;
    } finally {
      setLoading(false);
    }
  }, [endpoints, tablePicks, t]);

  const handleTypeOverrideChange = useCallback(
    (table: string, column: string, targetType: string) => {
      setTypeOverrides((prev) => {
        const filtered = prev.filter((o) => !(o.table === table && o.column === column));
        return [...filtered, { table, column, targetType }];
      });
    },
    [],
  );

  const buildPlan = useCallback(
    async (explicitOverrides?: ColumnTypeOverride[]) => {
      setError('');
      setDeployResult(null);
      const sourcePicks = tablePicks.filter((row) => row.enabled && row.origin !== 'target-only');
      const sourceTables = enabledSourceTableNames(tablePicks);
      const targetTables = sourcePicks.map(
        (row) => row.targetName ?? row.name.split('.').at(-1) ?? row.name,
      );
      const targetOnlyTables = enabledTargetOnlyTableNames(tablePicks);
      const sourceObjects = unifiedObjects.selectedSourceObjects;
      const targetObjects = unifiedObjects.selectedTargetObjects;
      if (
        sourceTables.length === 0 &&
        targetOnlyTables.length === 0 &&
        sourceObjects.length === 0 &&
        targetObjects.length === 0
      ) {
        setError(t('schemaDiff.selectionRequired'));
        return;
      }
      if (!endpoints.validateEndpoints()) return;

      setLoading(true);
      try {
        const srcConnId = await endpoints.ensureConnected('source');
        const tgtConnId = await endpoints.ensureConnected('target');
        if (!srcConnId || !tgtConnId) return;
        const overridesToUse = explicitOverrides ?? typeOverrides;
        const tablePlanParams = {
          sourceDbSessionId: srcConnId,
          targetDbSessionId: tgtConnId,
          tableNames: sourceTables,
          targetTableNames: targetTables,
          targetOnlyTableNames: targetOnlyTables.length > 0 ? targetOnlyTables : undefined,
          sourceSchema: endpoints.sourceSchema || undefined,
          targetSchema: endpoints.targetSchema || undefined,
          allowDestructive,
          includeIndexes,
          typeOverrides: overridesToUse.length > 0 ? overridesToUse : undefined,
        };
        const next =
          sourceObjects.length > 0 || targetObjects.length > 0
            ? await schemaDiffCommands.prepareUnifiedPlan({
                ...tablePlanParams,
                sourceObjects,
                targetObjects,
              })
            : await schemaDiffCommands.preparePlan(tablePlanParams);
        setPlan(next);
        setUseTransaction(dialectSupportsTransactionalDdl(next.targetDialect));
        setConfirmText('');
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        setLoading(false);
      }
    },
    [
      allowDestructive,
      endpoints,
      includeIndexes,
      tablePicks,
      t,
      typeOverrides,
      unifiedObjects.selectedSourceObjects,
      unifiedObjects.selectedTargetObjects,
    ],
  );

  const handleApplyTypeOverrides = useCallback(() => {
    void buildPlan(typeOverrides);
  }, [buildPlan, typeOverrides]);

  useEffect(() => {
    if (step !== 'plan') {
      planAutoRequestedRef.current = false;
      return;
    }
    if (plan || loading || planAutoRequestedRef.current) return;
    planAutoRequestedRef.current = true;
    void buildPlan();
  }, [step, plan, loading, buildPlan]);

  const transactionRequired = Boolean(
    plan?.statements.some((statement) => statement.requiresTransaction),
  );
  const transactionEnabled = transactionRequired || useTransaction;
  const deployAllowed = Boolean(
    plan &&
      !deployResult &&
      !plan.requirements?.length &&
      (!transactionRequired ||
        (transactionEnabled && dialectSupportsTransactionalDdl(plan.targetDialect))) &&
      (!requireRollback ||
        (transactionEnabled && dialectSupportsTransactionalDdl(plan.targetDialect))) &&
      canRunDeploy({
        hasDestructive: planHasDestructive(plan),
        confirmText,
        requireRollback,
        rollbackComplete: plan.rollbackCompleteness.complete,
        statementCount: plan.statements.length,
      }),
  );

  const handleDeploy = useCallback(async () => {
    if (!plan || !deployAllowed) return;
    setError('');
    setLoading(true);
    try {
      const tgtConnId = await endpoints.ensureConnected('target');
      if (!tgtConnId) return;
      const result = await schemaDiffCommands.executeDeploy({
        targetDbSessionId: tgtConnId,
        plan,
        useTransaction: transactionEnabled,
        requireRollback,
        confirmDestructive: planHasDestructive(plan) ? confirmText.trim() : undefined,
        targetDatabase: endpoints.targetDatabase || null,
        targetSchema: endpoints.targetSchema || null,
        profile: selectedSavedProfile
          ? { id: selectedSavedProfile.id, revision: selectedSavedProfile.updatedAt }
          : undefined,
      });
      setDeployResult(result);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, [
    confirmText,
    deployAllowed,
    endpoints,
    plan,
    requireRollback,
    selectedSavedProfile,
    transactionEnabled,
    useTransaction,
  ]);

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
    const next = SCHEMA_DIFF_STEPS[stepIndex + 1];
    if (step === 'objects' && next === 'compare') {
      if (
        endpoints.isCrossDialect &&
        (unifiedObjects.selectedSourceObjects.length > 0 ||
          unifiedObjects.selectedTargetObjects.length > 0)
      ) {
        setError(t('schemaDiff.crossDialectObjectBlocked'));
        return;
      }
      if (
        selectedTables.length === 0 &&
        unifiedObjects.selectedSourceObjects.length === 0 &&
        unifiedObjects.selectedTargetObjects.length === 0
      ) {
        setError(t('schemaDiff.selectionRequired'));
        return;
      }
      if (selectedTables.length === 0) {
        setStep('plan');
        return;
      }
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
  }, [
    step,
    stepIndex,
    runCompare,
    tablePicks.length,
    loadSourceTables,
    endpoints.isCrossDialect,
    selectedTables.length,
    unifiedObjects.selectedSourceObjects.length,
    unifiedObjects.selectedTargetObjects.length,
    t,
  ]);

  const goBack = () => {
    const prev = SCHEMA_DIFF_STEPS[stepIndex - 1];
    if (prev) setStep(prev);
  };

  const toggleTable = (name: string) => {
    setTablePicks((prev) =>
      prev.map((row) => (row.name === name ? { ...row, enabled: !row.enabled } : row)),
    );
  };

  const toggleUnifiedObject = (side: 'source' | 'target') => (object: SchemaDiffObjectIdentity) =>
    unifiedObjects.toggle(side, object);

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

  // All hooks have run. Do not evaluate lazy-domain translations until ready.
  if (!localesReady) {
    return <LocaleDomainLoading testId="schema-diff-locale-loading" />;
  }

  const endpointsCrossDialectNote = endpoints.isCrossDialect ? (
    <span
      data-testid="schema-diff-cross-dialect-note"
      className="mt-4 inline-block max-w-full rounded border border-edge bg-surface px-2 py-1 text-xs text-fg-muted"
    >
      {t('schemaDiff.crossDialectNote')}
    </span>
  ) : undefined;

  const planActions = (
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
      <Select
        className="min-w-36"
        triggerDataAttrs={{ 'data-testid': 'schema-diff-profile-select' }}
        value={savedSetups.selectedProfileId}
        title={t('schemaDiff.profileSelect')}
        placeholder={t('schemaDiff.profileSelect')}
        options={savedSetups.profiles.map((profile) => ({
          value: profile.id,
          label: profile.name,
        }))}
        onChange={savedSetups.setSelectedProfileId}
      />
      <Button
        variant="secondary"
        data-testid="schema-diff-profile-load"
        disabled={!savedSetups.selectedProfileId || loading}
        onClick={savedSetups.handleLoadProfile}
      >
        {t('schemaDiff.profileLoad')}
      </Button>
      <Button
        variant="secondary"
        data-testid="schema-diff-profile-save"
        onClick={savedSetups.openProfileSave}
      >
        {t('schemaDiff.profileSave')}
      </Button>
      <Button
        variant="ghost"
        data-testid="schema-diff-profile-delete"
        disabled={!savedSetups.selectedProfileId || loading}
        onClick={() => void savedSetups.handleDeleteProfile()}
      >
        {t('schemaDiff.profileDelete')}
      </Button>
      <Button
        variant="ghost"
        data-testid="schema-diff-export-config"
        onClick={() => void savedSetups.handleExportConfig()}
      >
        {clipboardFeedback === 'config'
          ? t('schemaDiff.configExported')
          : t('schemaDiff.exportConfig')}
      </Button>
      <Button
        variant="ghost"
        data-testid="schema-diff-import-config"
        onClick={savedSetups.openImportConfig}
      >
        {t('schemaDiff.importConfig')}
      </Button>
    </div>
  );

  return (
    <div
      className="flex h-screen min-h-0 flex-col bg-surface text-fg"
      data-testid="schema-diff-window"
    >
      <TitleBar
        title={t('common.schemaDiff')}
        rightContent={
          <div className="flex items-center gap-1">
            <MigrationRunHistoryDialog operation="schemaDiff" />
            <Button
              variant="secondary"
              className="h-6 w-6 !px-0"
              title={t('docs.openSchemaDiffHelp')}
              onClick={() => openDocsWindow('schemaDiff')}
            >
              <BookOpen className="h-3 w-3" />
            </Button>
          </div>
        }
      />

      <SchemaDiffWizardProgress step={step} />

      <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
        <div
          className={cn(
            'mx-auto flex min-h-0 w-full flex-1 flex-col px-6 py-6',
            SCHEMA_DIFF_NARROW_STEPS.includes(step) ? 'max-w-2xl overflow-auto' : 'max-w-6xl',
          )}
        >
          {(step === 'endpoints' || step === 'objects') && (
            <SchemaDiffSelectionSteps
              step={step}
              endpoints={endpoints}
              endpointsCrossDialectNote={endpointsCrossDialectNote}
              loading={objectsLoading}
              tablePicks={tablePicks}
              setTablePicks={setTablePicks}
              onToggleTable={toggleTable}
              unifiedObjects={unifiedObjects}
              onToggleUnifiedObject={toggleUnifiedObject}
              onClearObjectSelections={() => {
                unifiedObjects.selectAll('source', false);
                unifiedObjects.selectAll('target', false);
              }}
              onRetryObjects={() => void loadSourceTables()}
            />
          )}

          {(step === 'compare' || step === 'plan' || step === 'deploy') && (
            <SchemaDiffExecutionPanels
              step={step}
              loading={loading}
              diffs={diffs}
              selectedDiff={selectedDiff}
              selectedTables={selectedTables}
              selectedTable={selectedTable}
              onSelectTable={setSelectedTable}
              tableHasDiff={tableHasDiff}
              tableListWidth={tableListWidth}
              tableListResizeRef={tableListResizeRef}
              clipboardFeedback={clipboardFeedback}
              onCopySummary={() => void handleCopySummary()}
              plan={plan}
              planActions={planActions}
              allowDestructive={allowDestructive}
              onAllowDestructiveChange={setAllowDestructive}
              includeIndexes={includeIndexes}
              onIncludeIndexesChange={setIncludeIndexes}
              typeOverrides={typeOverrides}
              onTypeOverrideChange={handleTypeOverrideChange}
              onApplyTypeOverrides={handleApplyTypeOverrides}
              targetLabel={targetLabel}
              useTransaction={useTransaction}
              onUseTransactionChange={setUseTransaction}
              requireRollback={requireRollback}
              onRequireRollbackChange={setRequireRollback}
              confirmText={confirmText}
              onConfirmTextChange={setConfirmText}
              deployResult={deployResult}
              onRegenerate={() => void buildPlan()}
              onDeploy={() => void handleDeploy()}
            />
          )}

          {error && <CopyableError message={error} className="error-message mt-3" />}
        </div>
      </div>

      <div className="flex shrink-0 items-center justify-between border-t border-edge px-6 py-3">
        <Button variant="ghost" disabled={stepIndex === 0 || loading} onClick={goBack}>
          <ChevronLeft className="h-4 w-4" /> {t('schemaDiff.back')}
        </Button>
        <div className="flex items-center gap-2">
          {step === 'deploy' ? (
            <Button
              variant="run"
              data-testid="schema-diff-deploy"
              disabled={!deployAllowed || loading}
              onClick={() => void handleDeploy()}
            >
              {loading ? <Spinner size="lg" /> : t('schemaDiff.deploy')}
            </Button>
          ) : (
            <Button
              data-testid="schema-diff-next"
              disabled={!canNext || loading}
              onClick={() => void goNext()}
            >
              {loading ? <Spinner size="lg" /> : t('schemaDiff.next')}
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
      <SchemaDiffSavedSetupDialogs savedSetups={savedSetups} />
      <StatusBar left={<span className="truncate">{t('common.schemaDiff')}</span>} />
    </div>
  );
}
